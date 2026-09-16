package usecases

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/google/uuid"

	"github.com/allsource/control-plane/internal/application/usecases/billing"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// ScheduledTask represents a recurring task definition
type ScheduledTask struct {
	Name     string
	Interval time.Duration
	Enabled  bool
}

// OperationScheduler manages background operations (compaction, snapshots, etc.)
type OperationScheduler struct {
	operationRepo         repositories.OperationRepository
	auditRepo             repositories.AuditRepository
	coreClient            clients.CoreClient
	reportUsageUC         *billing.ReportUsageUseCase
	checkUsageWarnings    *billing.CheckUsageWarningsUseCase
	syncX402UsageUC       *billing.SyncX402UsageUseCase
	syncEventsUsageUC     *billing.SyncEventsUsageUseCase
	syncExtractionUsageUC *billing.SyncExtractionUsageUseCase
	syncSubsUC            *SyncSubscriptionsUseCase
	expireTrialsUC        *ExpireTrialsUseCase
	commsEfficiencyUC     *CommsEfficiencyUseCase
	tasks                 []ScheduledTask
	cancel                context.CancelFunc
	wg                    sync.WaitGroup
}

// NewOperationScheduler creates a new OperationScheduler.
func NewOperationScheduler(
	operationRepo repositories.OperationRepository,
	auditRepo repositories.AuditRepository,
	coreClient clients.CoreClient,
) *OperationScheduler {
	return &OperationScheduler{
		operationRepo: operationRepo,
		auditRepo:     auditRepo,
		coreClient:    coreClient,
		tasks: []ScheduledTask{
			{Name: "compaction", Interval: 6 * time.Hour, Enabled: true},
			{Name: "overage_reporting", Interval: 1 * time.Hour, Enabled: true},
			{Name: "usage_warnings", Interval: 1 * time.Hour, Enabled: true},
			// 1-minute reconciliation bounds how far a tenant can overshoot its
			// included x402 allowance between ticks; the in-process counter in
			// CoreQuotaChecker tightens it further within each instance.
			{Name: "x402_usage_sync", Interval: 1 * time.Minute, Enabled: true},
			// Reconcile events_used from the real per-tenant event count so the
			// quota meter (read by the usage bar AND usage enforcement) self-heals
			// from drift. Not hot-path (live increments keep it ~current); 5 min.
			{Name: "events_usage_sync", Interval: 5 * time.Minute, Enabled: true},
			{Name: "extraction_usage_sync", Interval: 5 * time.Minute, Enabled: true},
			// Self-heal subscription tier from LemonSqueezy in case a webhook was
			// missed or signature-rejected. Webhooks are primary; this is the
			// safety net so tiers don't go stale without manual replays.
			{Name: "subscription_sync", Interval: 15 * time.Minute, Enabled: true},
			// Suspend self-service trials whose 14-day window has elapsed without
			// converting to a paid plan (prompt 048). Hourly is plenty — a trial
			// is a 14-day box, so suspending within the hour of expiry is timely;
			// the suspend is reversible + audited (billing.trial.expired).
			{Name: "trial_expiry", Interval: 1 * time.Hour, Enabled: true},
			// Recompute the proactive-comms efficiency projection (funnel + causal
			// lift) by joining engagement events to goal events in Core (prompt 050).
			// 15 min: it's an operator read-model, not hot-path; the API also computes
			// live on demand, so staleness between ticks is cosmetic.
			{Name: "comms_efficiency", Interval: 15 * time.Minute, Enabled: true},
		},
	}
}

// SetReportUsageUseCase sets the overage reporting use case for the scheduler.
// Must be called before Start if overage_reporting task is enabled.
func (s *OperationScheduler) SetReportUsageUseCase(uc *billing.ReportUsageUseCase) {
	s.reportUsageUC = uc
}

// SetCheckUsageWarningsUseCase sets the usage warning use case for the scheduler.
// Must be called before Start if usage_warnings task is enabled.
func (s *OperationScheduler) SetCheckUsageWarningsUseCase(uc *billing.CheckUsageWarningsUseCase) {
	s.checkUsageWarnings = uc
}

// SetSyncX402UsageUseCase sets the x402 usage reconciliation use case for the
// scheduler. Must be called before Start if x402_usage_sync is enabled.
func (s *OperationScheduler) SetSyncX402UsageUseCase(uc *billing.SyncX402UsageUseCase) {
	s.syncX402UsageUC = uc
}

// SetSyncEventsUsageUseCase sets the events_used reconciliation use case.
// Must be called before Start if events_usage_sync is enabled.
func (s *OperationScheduler) SetSyncEventsUsageUseCase(uc *billing.SyncEventsUsageUseCase) {
	s.syncEventsUsageUC = uc
}

// SetSyncExtractionUsageUseCase sets the extraction-token usage reconciliation
// use case. Must be called before Start if extraction_usage_sync is enabled.
func (s *OperationScheduler) SetSyncExtractionUsageUseCase(uc *billing.SyncExtractionUsageUseCase) {
	s.syncExtractionUsageUC = uc
}

// SetSyncSubscriptionsUseCase sets the subscription reconciliation use case.
// Must be called before Start if subscription_sync is enabled.
func (s *OperationScheduler) SetSyncSubscriptionsUseCase(uc *SyncSubscriptionsUseCase) {
	s.syncSubsUC = uc
}

// SetExpireTrialsUseCase sets the trial-expiry sweep use case.
// Must be called before Start if trial_expiry is enabled.
func (s *OperationScheduler) SetExpireTrialsUseCase(uc *ExpireTrialsUseCase) {
	s.expireTrialsUC = uc
}

// SetCommsEfficiencyUseCase sets the proactive-comms efficiency reconciler.
// Must be called before Start if comms_efficiency is enabled.
func (s *OperationScheduler) SetCommsEfficiencyUseCase(uc *CommsEfficiencyUseCase) {
	s.commsEfficiencyUC = uc
}

// Start begins running scheduled tasks in the background.
func (s *OperationScheduler) Start(parentCtx context.Context) {
	ctx, cancel := context.WithCancel(parentCtx)
	s.cancel = cancel

	for _, task := range s.tasks {
		if !task.Enabled {
			continue
		}
		s.wg.Add(1)
		go s.runTask(ctx, task)
	}

	log.Printf("Operation scheduler started with %d tasks", len(s.tasks))
}

// Stop cancels all running tasks and waits for them to finish.
func (s *OperationScheduler) Stop() {
	if s.cancel != nil {
		s.cancel()
	}
	s.wg.Wait()
	log.Println("Operation scheduler stopped")
}

func (s *OperationScheduler) runTask(ctx context.Context, task ScheduledTask) {
	defer s.wg.Done()

	ticker := time.NewTicker(task.Interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			s.executeTask(ctx, task)
		}
	}
}

func (s *OperationScheduler) executeTask(ctx context.Context, task ScheduledTask) {
	switch task.Name {
	case "compaction":
		s.executeCompaction(ctx)
	case "overage_reporting":
		s.executeOverageReporting(ctx)
	case "usage_warnings":
		s.executeUsageWarnings(ctx)
	case "x402_usage_sync":
		s.executeX402UsageSync(ctx)
	case "events_usage_sync":
		s.executeEventsUsageSync(ctx)
	case "extraction_usage_sync":
		s.executeExtractionUsageSync(ctx)
	case "subscription_sync":
		s.executeSubscriptionSync(ctx)
	case "trial_expiry":
		s.executeTrialExpiry(ctx)
	case "comms_efficiency":
		s.executeCommsEfficiency(ctx)
	default:
		log.Printf("Unknown scheduled task: %s", task.Name)
	}
}

// executeCommsEfficiency recomputes the proactive-comms efficiency projection
// (funnel + causal lift) from Core events and persists it for the admin panel.
// No-op when the reconciler is not wired (no Core client).
func (s *OperationScheduler) executeCommsEfficiency(ctx context.Context) {
	if s.commsEfficiencyUC == nil {
		return
	}
	proj, err := s.commsEfficiencyUC.ExecuteAll(ctx)
	if err != nil {
		log.Printf("Scheduler: comms efficiency reconcile failed: %v", err)
		return
	}
	log.Printf("Scheduler: comms efficiency reconciled — groups=%d hero_sent=%d hero_converted=%d lift=%.3f",
		len(proj.Groups), proj.Hero.Sent, proj.Hero.Converted, proj.Hero.Lift)
}

func (s *OperationScheduler) executeCompaction(ctx context.Context) {
	if s.coreClient == nil {
		return
	}

	now := time.Now()
	op := &entities.Operation{
		ID:          uuid.New().String(),
		Type:        entities.OperationCompaction,
		Status:      entities.OperationPending,
		Parameters:  map[string]any{"scheduled": true},
		InitiatedBy: "scheduler",
		CreatedAt:   now,
	}

	if err := s.operationRepo.Save(op); err != nil {
		log.Printf("Scheduler: failed to save compaction operation: %v", err)
		return
	}

	// Transition to running
	op.Status = entities.OperationRunning
	op.StartedAt = &now
	_ = s.operationRepo.Update(op) //nolint:errcheck

	_, err := s.coreClient.TriggerCompaction(ctx, clients.CompactionRequest{Force: false})
	if err != nil {
		completedAt := time.Now()
		op.Status = entities.OperationFailed
		op.CompletedAt = &completedAt
		op.Error = err.Error()
		log.Printf("Scheduler: compaction failed: %v", err)
	} else {
		completedAt := time.Now()
		op.Status = entities.OperationCompleted
		op.CompletedAt = &completedAt
		op.Result = map[string]any{"triggered_by": "scheduler"}
		log.Println("Scheduler: compaction completed successfully")
	}

	_ = s.operationRepo.Update(op) //nolint:errcheck // best effort update

	// Audit log
	auditEvent, _ := entities.NewAuditEvent("operation.scheduled", "execute", "SCHEDULER", "/compaction") //nolint:errcheck
	auditEvent.WithResource("operation", op.ID)
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck // audit logging is non-critical
}

func (s *OperationScheduler) executeOverageReporting(ctx context.Context) {
	if s.reportUsageUC == nil {
		return
	}

	results := s.reportUsageUC.ExecuteAll(ctx)

	var reported, skipped, errored int
	for _, r := range results {
		switch {
		case r.Error != nil:
			errored++
			log.Printf("Scheduler: overage reporting failed for tenant %s: %v", r.TenantID, r.Error)
		case r.Skipped:
			skipped++
		default:
			reported++
		}
	}

	log.Printf("Scheduler: overage reporting complete — reported=%d skipped=%d errors=%d", reported, skipped, errored)

	// Audit log
	auditEvent, _ := entities.NewAuditEvent("billing.overage.scheduled_report", "execute", "SCHEDULER", "/billing/overage") //nolint:errcheck
	auditEvent.AddMetadata("reported", fmt.Sprintf("%d", reported))
	auditEvent.AddMetadata("skipped", fmt.Sprintf("%d", skipped))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}

// executeX402UsageSync reconciles each tenant's x402_used counter from the
// durable x402 events in Core, so the per-tier allowance depletes and overage
// kicks in (011). No-op when the use case is not wired.
func (s *OperationScheduler) executeX402UsageSync(ctx context.Context) {
	if s.syncX402UsageUC == nil {
		return
	}

	results := s.syncX402UsageUC.ExecuteAll(ctx)

	var synced, skipped, errored int
	for _, r := range results {
		switch {
		case r.Error != nil:
			errored++
			log.Printf("Scheduler: x402 usage sync failed for tenant %s: %v", r.TenantID, r.Error)
		case r.Skipped:
			skipped++
		default:
			synced++
		}
	}

	log.Printf("Scheduler: x402 usage sync complete — synced=%d skipped=%d errors=%d", synced, skipped, errored)

	auditEvent, _ := entities.NewAuditEvent("billing.x402.scheduled_sync", "execute", "SCHEDULER", "/billing/x402") //nolint:errcheck
	auditEvent.AddMetadata("synced", fmt.Sprintf("%d", synced))
	auditEvent.AddMetadata("skipped", fmt.Sprintf("%d", skipped))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}

// executeEventsUsageSync reconciles each tenant's events_used counter from the
// real per-tenant event count in Core, so the usage meter (read by the usage
// bar AND usage enforcement) self-heals from drift. No-op when not wired.
//
// This and executeExtractionUsageSync (and executeX402UsageSync above) are three
// sibling fan-outs that dupl reads as copies. They are NOT one function over one
// input: each drives a different use case, over a different result type
// (EventsUsageResult.EventsUsed vs ExtractionUsageResult.ExtractionTokensUsed),
// and emits a different audit action/resource and different log copy. The only
// shared part is the 4-line synced/skipped/errored tally; folding it out needs a
// generic helper plus a per-call adapter of the same size, in a file with no test
// coverage where a mistyped audit action would ship silently. Kept explicit.
//
//nolint:dupl // sibling-but-independent usage sync; see the note above
func (s *OperationScheduler) executeEventsUsageSync(ctx context.Context) {
	if s.syncEventsUsageUC == nil {
		return
	}

	results := s.syncEventsUsageUC.ExecuteAll(ctx)

	var synced, skipped, errored int
	for _, r := range results {
		switch {
		case r.Error != nil:
			errored++
			log.Printf("Scheduler: events usage sync failed for tenant %s: %v", r.TenantID, r.Error)
		case r.Skipped:
			skipped++
		default:
			synced++
			log.Printf("Scheduler: events_used corrected for tenant %s → %d", r.TenantID, r.EventsUsed)
		}
	}

	log.Printf("Scheduler: events usage sync complete — synced=%d skipped=%d errors=%d", synced, skipped, errored)

	auditEvent, _ := entities.NewAuditEvent("billing.events.scheduled_sync", "execute", "SCHEDULER", "/billing/events") //nolint:errcheck
	auditEvent.AddMetadata("synced", fmt.Sprintf("%d", synced))
	auditEvent.AddMetadata("skipped", fmt.Sprintf("%d", skipped))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}

// executeExtractionUsageSync reconciles each tenant's extraction_tokens_used
// meter from prime.extraction.usage events in Core. Record-only: extraction is
// sold as a fixed add-on, so the meter gates and displays but never bills.
//
//nolint:dupl // sibling-but-independent usage sync; see executeEventsUsageSync
func (s *OperationScheduler) executeExtractionUsageSync(ctx context.Context) {
	if s.syncExtractionUsageUC == nil {
		return
	}

	results := s.syncExtractionUsageUC.ExecuteAll(ctx)

	var synced, skipped, errored int
	for _, r := range results {
		switch {
		case r.Error != nil:
			errored++
			log.Printf("Scheduler: extraction usage sync failed for tenant %s: %v", r.TenantID, r.Error)
		case r.Skipped:
			skipped++
		default:
			synced++
			log.Printf("Scheduler: extraction_tokens_used updated for tenant %s → %d", r.TenantID, r.ExtractionTokensUsed)
		}
	}

	log.Printf("Scheduler: extraction usage sync complete — synced=%d skipped=%d errors=%d", synced, skipped, errored)

	auditEvent, _ := entities.NewAuditEvent("billing.extraction.scheduled_sync", "execute", "SCHEDULER", "/billing/extraction") //nolint:errcheck
	auditEvent.AddMetadata("synced", fmt.Sprintf("%d", synced))
	auditEvent.AddMetadata("skipped", fmt.Sprintf("%d", skipped))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}

// executeSubscriptionSync reconciles each tenant's tracked subscriptions against
// LemonSqueezy so a missed/rejected webhook can't leave a tier stale. No-op when
// the use case is not wired (no LS client configured).
func (s *OperationScheduler) executeSubscriptionSync(ctx context.Context) {
	if s.syncSubsUC == nil {
		return
	}

	results := s.syncSubsUC.ExecuteAll(ctx)

	var updated, errored int
	for _, r := range results {
		if r.Error != nil {
			errored++
			log.Printf("Scheduler: subscription sync failed for tenant %s: %v", r.TenantID, r.Error)
			continue
		}
		updated++
		log.Printf("Scheduler: subscription sync corrected tenant %s → tier=%s", r.TenantID, r.Tier)
	}

	if updated == 0 && errored == 0 {
		return // nothing drifted; stay quiet
	}

	log.Printf("Scheduler: subscription sync complete — updated=%d errors=%d", updated, errored)

	auditEvent, _ := entities.NewAuditEvent("billing.subscription.scheduled_sync", "execute", "SCHEDULER", "/billing/subscriptions") //nolint:errcheck
	auditEvent.AddMetadata("updated", fmt.Sprintf("%d", updated))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}

// executeTrialExpiry suspends self-service trial tenants whose 14-day window has
// elapsed and who have no active paid subscription (prompt 048). Reversible +
// audited; logs how many it actioned. No-op when the use case is not wired.
func (s *OperationScheduler) executeTrialExpiry(ctx context.Context) {
	if s.expireTrialsUC == nil {
		return
	}

	results := s.expireTrialsUC.ExecuteAll(ctx)

	var suspended, skipped, errored int
	for _, r := range results {
		switch {
		case r.Error != nil:
			errored++
			log.Printf("Scheduler: trial expiry failed for tenant %s: %v", r.TenantID, r.Error)
		case r.Suspended:
			suspended++
			log.Printf("Scheduler: trial expired — suspended tenant %s", r.TenantID)
		default:
			skipped++
		}
	}

	if suspended == 0 && errored == 0 {
		return // nothing expired; stay quiet
	}

	log.Printf("Scheduler: trial expiry complete — suspended=%d skipped=%d errors=%d", suspended, skipped, errored)

	auditEvent, _ := entities.NewAuditEvent("billing.trial.scheduled_expiry", "execute", "SCHEDULER", "/billing/trial-expiry") //nolint:errcheck
	auditEvent.AddMetadata("suspended", fmt.Sprintf("%d", suspended))
	auditEvent.AddMetadata("skipped", fmt.Sprintf("%d", skipped))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}

func (s *OperationScheduler) executeUsageWarnings(ctx context.Context) {
	if s.checkUsageWarnings == nil {
		return
	}

	results := s.checkUsageWarnings.ExecuteAll(ctx)

	var sent, skipped, errored int
	for _, r := range results {
		switch {
		case r.Error != nil:
			errored++
			log.Printf("Scheduler: usage warning failed for tenant %s: %v", r.TenantID, r.Error)
		case r.Skipped:
			skipped++
		case r.EmailSent:
			sent++
		default:
			skipped++
		}
	}

	log.Printf("Scheduler: usage warnings complete — sent=%d skipped=%d errors=%d", sent, skipped, errored)

	// Audit log
	auditEvent, _ := entities.NewAuditEvent("billing.usage_warning.scheduled_check", "execute", "SCHEDULER", "/billing/usage-warning") //nolint:errcheck
	auditEvent.AddMetadata("sent", fmt.Sprintf("%d", sent))
	auditEvent.AddMetadata("skipped", fmt.Sprintf("%d", skipped))
	auditEvent.AddMetadata("errors", fmt.Sprintf("%d", errored))
	_ = s.auditRepo.Log(auditEvent) //nolint:errcheck
}
