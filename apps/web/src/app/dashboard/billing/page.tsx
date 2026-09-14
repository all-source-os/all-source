"use client";

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { Calendar, Check, CreditCard, ExternalLink, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import useSWR from "swr";
import { PlanCards } from "@/components/billing/plan-cards";
import { UsageChart } from "@/components/billing/usage-chart";
import { FadeIn } from "@/components/ui/fade-in";
import { useDashboardStats } from "@/hooks/use-dashboard-stats";
import { apiClient } from "@/lib/api/client";
import { siteConfig } from "@/lib/config";
import {
  type Catalog,
  indexByTier,
  minimumAnnualSavingsPercent,
  PriceUnavailable,
  resolveBilledPrice,
  resolveYearlyPerMonth,
} from "@/lib/pricing-catalog";
import { useAuthStore } from "@/lib/stores/auth-store";
import { canonicalTier } from "@/lib/tier";

function getPlanConfig(tier: string) {
  // Normalize the raw backend subscription_tier to a canonical id at this edge,
  // then match purely on the canonical `tier` — no legacy billingTier matching.
  const canon = canonicalTier(tier);
  return siteConfig.pricing.find((p) => p.tier === canon) ?? siteConfig.pricing[0]!;
}

const paidTierIds = siteConfig.pricing
  .filter((plan) => !plan.isEnterprise && !plan.isSelfHost)
  .map((plan) => plan.tier);

async function loadBillingCatalog(): Promise<Catalog | null> {
  const response = await fetch("/api/billing/catalog");
  if (!response.ok) return null;
  const catalog = (await response.json()) as Catalog;
  return catalog?.tiers?.length ? catalog : null;
}

export default function BillingPage() {
  const { tenant, user } = useAuthStore();
  const { stats } = useDashboardStats();
  const [isYearly, setIsYearly] = useState(false);
  const [requestedTier, setRequestedTier] = useState<string | null>(null);
  const [isLoadingPortal, setIsLoadingPortal] = useState(false);
  // The tier whose checkout is currently being created (button shows a spinner).
  const [upgradingTier, setUpgradingTier] = useState<string | null>(null);
  // Set after returning from checkout / an in-place plan change.
  const [checkoutSuccess, setCheckoutSuccess] = useState(false);
  const [planChanged, setPlanChanged] = useState(false);
  // User-visible error when a checkout/plan-change fails, so the button doesn't
  // just spin "Redirecting…" then silently do nothing.
  const [planError, setPlanError] = useState<string | null>(null);
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get("period") === "annual") setIsYearly(true);
    const requestedPlan = params.get("plan");
    if (["indie", "studio", "scale"].includes(requestedPlan ?? "")) {
      setRequestedTier(requestedPlan);
    }
    if (params.get("checkout") === "success") setCheckoutSuccess(true);
    if (params.get("changed") === "success") setPlanChanged(true);
    if (params.has("checkout") || params.has("changed")) {
      // Clean the param so a refresh doesn't keep showing the banner.
      window.history.replaceState(null, "", window.location.pathname);
    }
  }, []);
  // Live LemonSqueezy prices (source of truth) fetched via the catalog proxy.
  const { data: catalog = null } = useSWR("/api/billing/catalog", loadBillingCatalog, {
    dedupingInterval: 2_000,
    refreshInterval: (latest) => (latest?.tiers?.length ? 300_000 : 5_000),
    revalidateOnFocus: false,
  });
  const yearlySavings = minimumAnnualSavingsPercent(catalog, paidTierIds);

  // Canonical tier id (self-host | indie | studio | scale | enterprise) — the
  // raw backend value is normalized once here so all comparisons below are
  // against canonical ids ("self-host" is the free tier).
  const currentTier = canonicalTier(tenant?.subscription_tier);
  const planConfig = getPlanConfig(currentTier);
  const eventsUsed = stats.events.used || tenant?.events_used || 0;
  const eventsQuota = stats.events.quota || tenant?.events_quota || 10000;
  const queriesUsed = stats.queries.used || tenant?.queries_used || 0;
  const queriesQuota = stats.queries.quota || tenant?.queries_quota || 10000;
  const trialEndsAt = tenant?.trial_ends_at;
  const subscriptionEndsAt = tenant?.subscription_ends_at;

  const currentCat = indexByTier(catalog)[planConfig.tier];
  const isAnnualPlan = tenant?.billing_period === "annual";
  const displayPrice = resolveBilledPrice(
    currentCat,
    planConfig.price,
    isAnnualPlan ? "annual" : "monthly"
  );
  const annualEquivalent = isAnnualPlan
    ? resolveYearlyPerMonth(currentCat, planConfig.price)
    : null;

  const handleManageSubscription = async () => {
    setIsLoadingPortal(true);
    try {
      const response = await apiClient.getBillingPortal(tenant?.id);
      if (response.data?.portal_url) {
        window.open(response.data.portal_url, "_blank");
      }
    } catch (error) {
      console.error("Failed to get billing portal:", error);
    } finally {
      setIsLoadingPortal(false);
    }
  };

  // Only the self-serve paid tiers (indie/studio/scale) have a real LemonSqueezy
  // subscription that can be swapped IN PLACE via change-plan. Enterprise is a
  // manual/sales-led override with NO LS subscription, and free/self-host has
  // none — for those, "change tier" must start a fresh CHECKOUT, not change-plan
  // (which 500s with "no active subscription to change"). This was the bug: an
  // Enterprise tenant clicking Downgrade hit change-plan and got a 500.
  const hasActiveSubscription = (["indie", "studio", "scale"] as const).includes(
    currentTier as "indie" | "studio" | "scale"
  );

  const handleUpgrade = async (
    planTier: string,
    billingPeriod: "monthly" | "annual" = "monthly"
  ) => {
    if (planTier === "enterprise") {
      window.open("mailto:sales@all-source.xyz?subject=Enterprise%20Plan%20Inquiry", "_blank");
      return;
    }
    setUpgradingTier(planTier);
    setPlanError(null);

    // Existing self-serve subscriber → in-place plan change.
    if (hasActiveSubscription) {
      try {
        const response = await apiClient.changePlan(planTier, billingPeriod);
        if (response.data?.tier) {
          // Tier applied server-side; reload so the dashboard reflects it.
          window.location.href = "/dashboard/billing?changed=success";
          return;
        }
        setUpgradingTier(null);
        setPlanError(response.error?.message || "Couldn't change your plan. Please try again.");
      } catch (error) {
        setUpgradingTier(null);
        console.error("Failed to change plan:", error);
        setPlanError("Couldn't change your plan. Please try again or contact support.");
      }
      return;
    }

    // No active LS subscription (enterprise/free/self-host) → new checkout.
    try {
      const response = await apiClient.createCheckout(planTier, billingPeriod, {
        tenantId: tenant?.id,
        email: user?.email,
        redirectUrl: `${window.location.origin}/dashboard/billing?checkout=success`,
      });
      if (response.data?.checkout_url) {
        window.location.href = response.data.checkout_url;
        return; // navigating away; keep the spinner until the redirect happens
      }
      setUpgradingTier(null);
      setPlanError(response.error?.message || "Couldn't start checkout. Please try again.");
    } catch (error) {
      setUpgradingTier(null);
      console.error("Failed to create checkout:", error);
      setPlanError("Couldn't start checkout. Please try again or contact support.");
    }
  };

  const formatDate = (dateStr: string | null) => {
    if (!dateStr) return null;
    return new Date(dateStr).toLocaleDateString("en-US", {
      month: "long",
      day: "numeric",
      year: "numeric",
    });
  };

  return (
    <div className="space-y-8">
      {/* Post-checkout confirmation — the plan updates asynchronously once the
          LemonSqueezy webhook lands, so set expectations explicitly. */}
      {checkoutSuccess && (
        <div className="flex items-start gap-3 rounded-lg border border-primary/30 bg-primary/10 p-4">
          <Check className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
          <div className="text-sm">
            <p className="font-semibold text-foreground">Payment received — thank you!</p>
            <p className="mt-1 text-muted-foreground">
              Your plan upgrades automatically within a few minutes once payment is confirmed. No
              action needed — refresh this page to see the change.
            </p>
          </div>
          <button
            type="button"
            onClick={() => setCheckoutSuccess(false)}
            className="ml-auto text-muted-foreground hover:text-foreground"
            aria-label="Dismiss"
          >
            <span aria-hidden>×</span>
          </button>
        </div>
      )}

      {planChanged && (
        <div className="flex items-start gap-3 rounded-lg border border-primary/30 bg-primary/10 p-4">
          <Check className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
          <div className="text-sm">
            <p className="font-semibold text-foreground">Plan updated ✓</p>
            <p className="mt-1 text-muted-foreground">
              Your subscription was changed in place — the new plan is active now (prorated by your
              billing provider).
            </p>
          </div>
          <button
            type="button"
            onClick={() => setPlanChanged(false)}
            className="ml-auto text-muted-foreground hover:text-foreground"
            aria-label="Dismiss"
          >
            <span aria-hidden>×</span>
          </button>
        </div>
      )}

      {/* Header */}
      <FadeIn delay={0.1} inView>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h1 className="text-2xl font-bold tracking-tight md:text-3xl">Billing & Usage</h1>
            <p className="mt-1 text-muted-foreground">Manage your subscription and monitor usage</p>
          </div>
          {currentTier !== "self-host" && (
            <Button variant="outline" onClick={handleManageSubscription} disabled={isLoadingPortal}>
              {isLoadingPortal ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <CreditCard className="mr-2 h-4 w-4" />
              )}
              Manage Subscription
              <ExternalLink className="ml-2 h-4 w-4" />
            </Button>
          )}
        </div>
      </FadeIn>

      {/* Trial/Subscription Banner */}
      {trialEndsAt && (
        <FadeIn delay={0.15} inView>
          <Card className="border-yellow-500/50 bg-yellow-500/10">
            <CardContent className="flex items-center justify-between p-4">
              <div className="flex items-center gap-3">
                <Calendar className="h-5 w-5 text-yellow-600" />
                <div>
                  <p className="font-medium text-yellow-600">Trial Period</p>
                  <p className="text-sm text-yellow-600/80">
                    Your trial ends on {formatDate(trialEndsAt)}. Upgrade to continue.
                  </p>
                </div>
              </div>
              <Button size="sm" onClick={() => handleUpgrade("studio")}>
                Upgrade Now
              </Button>
            </CardContent>
          </Card>
        </FadeIn>
      )}

      {/* Current Plan Card */}
      <FadeIn delay={0.2} inView>
        <Card>
          <CardHeader className="flex flex-row items-center justify-between">
            <div>
              <CardTitle>Current Plan</CardTitle>
              <CardDescription>Your active subscription details</CardDescription>
            </div>
            <Badge
              variant={currentTier === "self-host" ? "secondary" : "default"}
              className="text-sm"
            >
              {planConfig.name}
            </Badge>
          </CardHeader>
          <CardContent>
            <div className="grid gap-6 md:grid-cols-3">
              {/* Plan details */}
              <div className="space-y-4">
                <div>
                  <p className="text-sm text-muted-foreground">
                    {isAnnualPlan ? "Annual list price" : "Monthly list price"}
                  </p>
                  <p className="text-2xl font-bold">
                    {displayPrice}
                    {displayPrice !== "Custom" && displayPrice !== PriceUnavailable && (
                      <span className="text-base font-normal text-muted-foreground">
                        /{isAnnualPlan ? "year" : "month"}
                      </span>
                    )}
                  </p>
                  {isAnnualPlan &&
                    currentTier !== "self-host" &&
                    displayPrice !== PriceUnavailable && (
                      <p className="text-xs text-muted-foreground">
                        {annualEquivalent}/month equivalent · charged annually
                      </p>
                    )}
                  {displayPrice !== "Custom" && displayPrice !== PriceUnavailable && (
                    <p className="text-xs text-muted-foreground">
                      Check your invoice for discounts and tax.
                    </p>
                  )}
                </div>
                {tenant?.billing_period && currentTier !== "self-host" && (
                  <div>
                    <p className="text-sm text-muted-foreground">Billing Period</p>
                    <p className="font-medium capitalize">{tenant.billing_period}</p>
                  </div>
                )}
                {subscriptionEndsAt && (
                  <div>
                    <p className="text-sm text-muted-foreground">Billing Period Ends</p>
                    <p className="font-medium">{formatDate(subscriptionEndsAt)}</p>
                  </div>
                )}
                {currentTier === "self-host" && (
                  <Button size="sm" onClick={() => handleUpgrade("studio")}>
                    Upgrade
                  </Button>
                )}
              </div>

              {/* Plan features from config */}
              <div className="col-span-2 space-y-2">
                <p className="text-sm font-medium">Plan Features</p>
                <div className="grid grid-cols-2 gap-2">
                  {planConfig.features.map((feature) => (
                    <div key={feature} className="flex items-center gap-2 text-sm">
                      <Check className="h-4 w-4 text-primary" />
                      <span>{feature}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </FadeIn>

      {/* Usage Charts */}
      <FadeIn delay={0.3} inView>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold">Usage This Month</h2>
            <span className="text-sm text-muted-foreground">Resets on the 1st of each month</span>
          </div>
          <div className="grid gap-6 md:grid-cols-2">
            <UsageChart title="Events" used={eventsUsed} quota={eventsQuota} />
            <UsageChart title="Queries" used={queriesUsed} quota={queriesQuota} />
          </div>
        </div>
      </FadeIn>

      {/* Upgrade Section */}
      <FadeIn delay={0.4} inView>
        <div className="space-y-4">
          <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
            <div>
              <h2 className="text-lg font-semibold">Available Plans</h2>
              <p className="text-sm text-muted-foreground">Choose the plan that fits your needs</p>
            </div>

            {planError && (
              <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {planError}
              </p>
            )}

            {/* Billing toggle */}
            <div className="flex items-center gap-2 rounded-lg border border-border p-1">
              <button
                type="button"
                onClick={() => setIsYearly(false)}
                className={cn(
                  "rounded-md px-3 py-1.5 text-sm font-medium transition-all",
                  !isYearly ? "bg-primary text-primary-foreground" : "hover:bg-muted"
                )}
              >
                Monthly
              </button>
              <button
                type="button"
                onClick={() => setIsYearly(true)}
                className={cn(
                  "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-all",
                  isYearly ? "bg-primary text-primary-foreground" : "hover:bg-muted"
                )}
              >
                Yearly
                {yearlySavings !== null && (
                  <span className="rounded-full bg-emerald-950 px-2 py-0.5 text-xs font-semibold text-emerald-100">
                    Save {yearlySavings}%
                  </span>
                )}
              </button>
            </div>
          </div>

          <PlanCards
            currentPlan={currentTier}
            isYearly={isYearly}
            selectedTier={requestedTier}
            catalog={catalog}
            loadingTier={upgradingTier}
            onUpgrade={handleUpgrade}
          />
          {catalog?.stale && catalog.fetched_at && (
            <p className="text-sm text-muted-foreground" role="status">
              Prices last confirmed with Lemon Squeezy {catalog.fetched_at.slice(0, 10)}. Checkout
              shows the current total before payment.
            </p>
          )}
        </div>
      </FadeIn>

      {/* FAQ */}
      <FadeIn delay={0.5} inView>
        <Card>
          <CardHeader>
            <CardTitle>Billing FAQ</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div>
              <h3 className="font-medium">What happens if I exceed my quota?</h3>
              <p className="text-sm text-muted-foreground">
                You can enable overage billing to continue using the service beyond your quota.
                Overages are charged at a per-unit rate based on your plan.
              </p>
            </div>
            <div>
              <h3 className="font-medium">Can I downgrade my plan?</h3>
              <p className="text-sm text-muted-foreground">
                Yes, you can downgrade at any time. The change will take effect at the end of your
                current billing period.
              </p>
            </div>
            <div>
              <h3 className="font-medium">Do you offer refunds?</h3>
              <p className="text-sm text-muted-foreground">
                We offer a 14-day money-back guarantee for all paid plans. Contact support if you'd
                like a refund.
              </p>
            </div>
          </CardContent>
        </Card>
      </FadeIn>
    </div>
  );
}
