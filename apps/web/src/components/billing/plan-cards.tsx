"use client";

import { Button, Card, CardContent, CardHeader, CardTitle } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { ArrowDown, Check, Loader2, Sparkles } from "lucide-react";
import { siteConfig } from "@/lib/config";
import {
  type Catalog,
  indexByTier,
  PriceUnavailable,
  resolveAnnualTotal,
  resolveBilledPrice,
  resolveYearlyPerMonth,
} from "@/lib/pricing-catalog";
import { type CanonicalTier, canonicalTier, TIER_RANK } from "@/lib/tier";

interface PlanCardsProps {
  currentPlan?: string;
  isYearly?: boolean;
  catalog?: Catalog | null;
  /** Tier whose checkout is being created — its button shows a spinner. */
  loadingTier?: string | null;
  selectedTier?: string | null;
  onUpgrade?: (planName: string, billingPeriod: "monthly" | "annual") => void;
}

export function PlanCards({
  currentPlan = "free",
  isYearly = false,
  catalog,
  loadingTier,
  selectedTier,
  onUpgrade,
}: PlanCardsProps) {
  // Dashboard only offers checkout for tiers with a backend billing tier.
  // Self-Host has no checkout; surface it on /pricing instead.
  const plans = siteConfig.pricing.filter((p) => !p.isSelfHost);
  // Normalize the incoming subscription_tier to a canonical id once.
  const currentCanon = canonicalTier(currentPlan);
  const currentRank = TIER_RANK[currentCanon];
  // Live LemonSqueezy prices (source of truth) keyed by public tier id.
  const prices = indexByTier(catalog ?? null);

  return (
    <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-4">
      {plans.map((plan) => {
        // Canonical tier ids only — the dashboard matches/ranks on plan.tier.
        const planRank = TIER_RANK[plan.tier as CanonicalTier] ?? 0;
        const isCurrent = plan.tier === currentCanon;
        const isRequested = plan.tier === selectedTier && !isCurrent;
        const isAbove = planRank > currentRank;
        const isBelow = planRank < currentRank;
        const isPopular = plan.isPopular;
        const isUpgrading = !!loadingTier && loadingTier === plan.tier;
        const cat = prices[plan.tier];
        // Paid tiers with no live/cached price render a dash, never a config number.
        const displayPrice = resolveBilledPrice(cat, plan.price, isYearly ? "annual" : "monthly");
        const annualTotal = resolveAnnualTotal(cat);
        const yearlyEquivalent = resolveYearlyPerMonth(cat, plan.price);

        return (
          <Card
            key={plan.name}
            className={cn(
              // flex-col + h-full so every card fills its grid row and the CTA
              // pins to the bottom regardless of how many features it lists.
              "relative flex h-full flex-col overflow-hidden transition-all",
              isPopular && "border-primary shadow-lg",
              isRequested && "ring-2 ring-primary",
              isCurrent && "ring-2 ring-primary"
            )}
          >
            {/* Popular badge */}
            {isPopular && !isCurrent && (
              <div className="absolute -right-8 top-4 rotate-45 bg-primary px-10 py-1 text-xs font-medium text-primary-foreground">
                Popular
              </div>
            )}

            {/* Current badge */}
            {isCurrent && (
              <div className="absolute left-4 top-4">
                <span className="rounded-full bg-primary/10 px-3 py-1 text-xs font-medium text-primary">
                  Current Plan
                </span>
              </div>
            )}

            {isRequested && (
              <div className="absolute left-4 top-4">
                <span className="rounded-full bg-primary/10 px-3 py-1 text-xs font-medium text-primary">
                  Selected from pricing
                </span>
              </div>
            )}

            <CardHeader className={cn("pb-2", (isCurrent || isRequested) && "pt-12")}>
              <CardTitle className="text-sm font-medium text-muted-foreground">
                {plan.name}
              </CardTitle>
              <div className="flex items-baseline gap-1">
                <span className="text-4xl font-bold">{displayPrice}</span>
                {displayPrice !== "Custom" && displayPrice !== PriceUnavailable && (
                  <span className="text-muted-foreground">/{isYearly ? "year" : "month"}</span>
                )}
              </div>
              {isYearly && plan.price !== "$0" && plan.price !== "Custom" && (
                <p className="text-xs text-muted-foreground">
                  {annualTotal
                    ? `${yearlyEquivalent}/mo equivalent · charged annually`
                    : "Annual price unavailable"}
                </p>
              )}
              <p className="text-sm text-muted-foreground">{plan.description}</p>
            </CardHeader>

            <CardContent className="flex flex-1 flex-col space-y-4">
              {/* Features */}
              <ul className="space-y-2">
                {plan.features.map((feature) => (
                  <li key={feature} className="flex items-center gap-2 text-sm">
                    <Check className="h-4 w-4 shrink-0 text-primary" />
                    <span>{feature}</span>
                  </li>
                ))}
              </ul>

              {/* CTA — mt-auto pins it to the card bottom across varying heights */}
              <div className="mt-auto pt-2">
                {isCurrent ? (
                  <Button className="w-full" variant="outline" disabled>
                    Current Plan
                  </Button>
                ) : plan.isEnterprise ? (
                  <Button
                    className="w-full"
                    variant="outline"
                    onClick={() => onUpgrade?.(plan.tier, isYearly ? "annual" : "monthly")}
                  >
                    Contact Sales
                  </Button>
                ) : isAbove ? (
                  <Button
                    className="w-full transition-all hover:brightness-110 active:scale-[0.98] disabled:opacity-70"
                    variant={isPopular ? "default" : "outline"}
                    disabled={isUpgrading}
                    onClick={() => onUpgrade?.(plan.tier, isYearly ? "annual" : "monthly")}
                  >
                    {isUpgrading ? (
                      <>
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                        Redirecting…
                      </>
                    ) : (
                      <>
                        {isPopular && <Sparkles className="mr-2 h-4 w-4" />}
                        Upgrade
                      </>
                    )}
                  </Button>
                ) : isBelow && plan.tier !== "self-host" ? (
                  <Button
                    className="w-full transition-all hover:brightness-110 active:scale-[0.98] disabled:opacity-70"
                    variant="ghost"
                    disabled={isUpgrading}
                    onClick={() => onUpgrade?.(plan.tier, isYearly ? "annual" : "monthly")}
                  >
                    {isUpgrading ? (
                      <>
                        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                        Redirecting…
                      </>
                    ) : (
                      <>
                        <ArrowDown className="mr-2 h-4 w-4" />
                        Downgrade
                      </>
                    )}
                  </Button>
                ) : (
                  // Free plan when user is on a paid plan — no button, cancel via portal
                  <p className="text-center text-xs text-muted-foreground">
                    Cancel via Manage Subscription
                  </p>
                )}
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
