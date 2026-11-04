package usecases

import (
	"context"
	"time"

	"github.com/google/uuid"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// StartReplayUseCase orchestrates starting a replay via Core with local operation tracking.
type StartReplayUseCase struct {
	opRepo     repositories.OperationRepository
	auditRepo  repositories.AuditRepository
	coreClient clients.CoreClient
}

// NewStartReplayUseCase creates a new StartReplayUseCase.
func NewStartReplayUseCase(
	opRepo repositories.OperationRepository,
	auditRepo repositories.AuditRepository,
	coreClient clients.CoreClient,
) *StartReplayUseCase {
	return &StartReplayUseCase{
		opRepo:     opRepo,
		auditRepo:  auditRepo,
		coreClient: coreClient,
	}
}

// Execute starts a replay: creates pending Operation -> calls Core -> stores replay ID in Operation -> audit log.
func (uc *StartReplayUseCase) Execute(ctx context.Context, entityID string, asOf *time.Time, tenantID, initiatedBy string) (*entities.Operation, error) {
	now := time.Now()
	params := map[string]any{"entity_id": entityID}
	if asOf != nil {
		params["as_of"] = asOf.Format(asOfLayout)
	}

	op := &entities.Operation{
		ID:          uuid.New().String(),
		Type:        entities.OperationReplay,
		Status:      entities.OperationPending,
		TenantID:    tenantID,
		Parameters:  params,
		InitiatedBy: initiatedBy,
		CreatedAt:   now,
	}

	if err := uc.opRepo.Save(op); err != nil {
		return nil, err
	}

	// Transition to running
	op.Status = entities.OperationRunning
	op.StartedAt = &now
	_ = uc.opRepo.Update(op) //nolint:errcheck // best-effort status update

	// Call Core to start replay
	resp, err := uc.coreClient.StartReplay(ctx, clients.ReplayRequest{
		EntityID: entityID,
		AsOf:     asOf,
	})
	if err != nil {
		completedAt := time.Now()
		op.Status = entities.OperationFailed
		op.CompletedAt = &completedAt
		op.Error = err.Error()
		_ = uc.opRepo.Update(op) //nolint:errcheck // best-effort status update
		return op, err
	}

	// Store replay ID in operation result — replay is still running, not completed yet
	op.Result = map[string]any{
		"replay_id": resp.ReplayID,
		"status":    resp.Status,
	}
	_ = uc.opRepo.Update(op) //nolint:errcheck // best-effort status update

	// Audit log
	auditEvent, _ := entities.NewAuditEvent("operation.replay_started", "start_replay", "POST", "/operations/replay") //nolint:errcheck
	auditEvent.WithResource("operation", op.ID).WithTenant(tenantID)
	_ = uc.auditRepo.Log(auditEvent) //nolint:errcheck // audit logging is non-critical

	return op, nil
}
