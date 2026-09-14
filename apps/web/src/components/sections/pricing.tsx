"use client";

import { buttonVariants, cn, Section } from "@allsource/ui";
import { Check } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { FaStar } from "react-icons/fa";
import useSWR from "swr";
import { staticMotion as motion } from "@/components/ui/static-motion";
import { siteConfig } from "@/lib/config";
import {
  type Catalog,
  indexByTier,
  PriceUnavailable,
  pricingSignupHref,
  resolveAnnualTotal,
  resolveBilledPrice,
  resolveYearlyPerMonth,
} from "@/lib/pricing-catalog";

// Cards shown in the top row: the paid hosted tiers only. Enterprise renders as
// a full-width strip below (PRICING_EXPOSURE_PLAN.md §3); Self-Host is no longer
// shown as a $0 marketing card — the product has no free plan, so the entry tier
// on /pricing is Indie. (The Apache-2.0 self-host path still lives in the "Why no free
// plan?" FAQ.) The 14-day trial is how you start without paying immediately.
const cardTiers = siteConfig.pricing.filter((p) => !p.isEnterprise && !p.isSelfHost);
const enterpriseTier = siteConfig.pricing.find((p) => p.isEnterprise);

async function loadCatalog(): Promise<Catalog | null> {
  const response = await fetch("/api/billing/catalog");
  if (!response.ok) throw new Error("Pricing catalog unavailable");
  const catalog = (await response.json()) as Catalog;
  return catalog?.tiers?.length ? catalog : null;
}

// `catalog` carries live LemonSqueezy prices (source of truth). On a statically
// rendered page, load them after paint; never substitute static paid prices.
export default function PricingSection({
  catalog,
  headingLevel = 2,
  title = "Pricing",
}: {
  catalog?: Catalog | null;
  headingLevel?: 1 | 2;
  title?: string;
}) {
  const [isMonthly, setIsMonthly] = useState(true);
  const { data: browserCatalog, isLoading: catalogLoading } = useSWR(
    catalog ? null : "/api/billing/catalog",
    loadCatalog,
    {
      dedupingInterval: 300_000,
      revalidateOnFocus: false,
    }
  );
  const prices = indexByTier(catalog ?? browserCatalog ?? null);

  return (
    <Section
      title={title}
      headingLevel={headingLevel}
      subtitle="Pay for the events your agents write. Start with a 14-day trial — no charge until it ends."
    >
      {/* Billing toggle — single unambiguous label per state. */}
      <div className="flex justify-center mb-10">
        <div className="inline-flex items-center rounded-full border-2 border-primary/20 bg-muted/50 p-1">
          <button
            type="button"
            onClick={() => setIsMonthly(true)}
            className={cn(
              "px-4 py-2 rounded-full text-sm font-semibold transition-all duration-200",
              isMonthly
                ? "bg-primary text-primary-foreground shadow-md"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Monthly
          </button>
          <button
            type="button"
            onClick={() => setIsMonthly(false)}
            className={cn(
              "px-4 py-2 rounded-full text-sm font-semibold transition-all duration-200 flex items-center gap-1",
              !isMonthly
                ? "bg-primary text-primary-foreground shadow-md"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            Yearly
          </button>
        </div>
      </div>

      {/* Three paid card tiers (self-host removed) — a 3-col grid, centered and
          width-capped so the cards don't left-align against an empty 4th column. */}
      <div className="mx-auto grid max-w-5xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {cardTiers.map((plan, index) => {
          // LemonSqueezy price (source of truth). Paid tiers with no live/
          // cached price render a dash, never a possibly-stale config number.
          const cat = prices[plan.tier];
          const yearlyEquivalent = resolveYearlyPerMonth(cat, plan.price);
          const annualTotal = resolveAnnualTotal(cat);
          const displayPrice = resolveBilledPrice(
            cat,
            plan.price,
            isMonthly ? "monthly" : "annual"
          );
          const isNumericPrice = displayPrice !== PriceUnavailable;
          const isPriceUnavailable = displayPrice === PriceUnavailable;

          return (
            <motion.div
              key={plan.name}
              initial={{ y: 50, opacity: 1 }}
              whileInView={{ y: 0, opacity: 1 }}
              viewport={{ once: true }}
              transition={{
                duration: 1.6,
                type: "spring",
                stiffness: 100,
                damping: 30,
                delay: 0.4 + index * 0.05,
                opacity: { duration: 0.5 },
              }}
              className={cn(
                "rounded-2xl border-[1px] p-6 text-center lg:flex lg:flex-col lg:justify-between relative",
                plan.isPopular
                  ? "border-primary border-[2px] bg-primary/[0.04]"
                  : "border-border bg-background",
                plan.isSelfHost && "border-dashed"
              )}
            >
              {plan.isPopular && (
                <div className="absolute top-0 right-0 bg-primary py-0.5 px-2 rounded-bl-xl rounded-tr-xl flex items-center">
                  <FaStar className="text-primary-foreground" />
                  <span className="ml-1 font-sans font-semibold text-primary-foreground">
                    Popular
                  </span>
                </div>
              )}
              <div>
                <p className="text-base font-semibold text-muted-foreground">{plan.name}</p>
                <p className="mt-6 flex items-end justify-center gap-x-1">
                  <span
                    className={cn(
                      "font-bold tracking-tight text-foreground",
                      isPriceUnavailable ? "text-2xl" : "text-5xl"
                    )}
                  >
                    {displayPrice}
                  </span>
                  {isNumericPrice && (
                    <span className="mb-1 text-sm font-semibold leading-6 tracking-wide text-muted-foreground">
                      /{isMonthly ? "mo" : "yr"}
                    </span>
                  )}
                </p>
                <p className="text-xs leading-5 text-muted-foreground">
                  {plan.isSelfHost
                    ? plan.period
                    : isPriceUnavailable
                      ? catalogLoading
                        ? "Checking current price"
                        : "Live catalog could not be reached"
                      : isNumericPrice
                        ? isMonthly
                          ? "Billed monthly"
                          : annualTotal
                            ? `${yearlyEquivalent}/mo equivalent · charged annually`
                            : "Annual price unavailable"
                        : ""}
                </p>

                {plan.x402 && (
                  <p className="mt-4 text-xs leading-5 text-muted-foreground">
                    Includes {plan.x402.included}. {plan.x402.overage}.
                  </p>
                )}

                <ul className="mt-5 gap-2 flex flex-col text-left">
                  {plan.features.map((feature) => (
                    <li key={feature} className="flex items-start">
                      <Check className="mr-2 mt-0.5 h-4 w-4 shrink-0 text-primary" />
                      <span className="text-sm">{feature}</span>
                    </li>
                  ))}
                </ul>
              </div>

              <div>
                <hr className="w-full my-4" />
                {/* Sign-up preserves selected tier and billing period. */}
                <Link
                  href={pricingSignupHref(plan.tier, isMonthly ? "monthly" : "annual")}
                  className={cn(
                    buttonVariants({ variant: "outline" }),
                    "group relative w-full gap-2 overflow-hidden text-base font-semibold tracking-tight",
                    "transform-gpu ring-offset-current transition-all duration-300 ease-out hover:ring-2 hover:ring-primary hover:ring-offset-1 hover:bg-primary hover:text-primary-foreground",
                    plan.isPopular
                      ? "!bg-[#075985] !text-white hover:!bg-[#064E75]"
                      : "bg-background text-foreground"
                  )}
                >
                  {plan.buttonText}
                </Link>
                <p className="mt-6 text-xs leading-5 text-muted-foreground">{plan.description}</p>
              </div>
            </motion.div>
          );
        })}
      </div>

      {/* Enterprise — quieter strip below, width-matched to the cards above. */}
      {enterpriseTier && (
        <div className="mx-auto mt-6 flex max-w-5xl flex-col items-start justify-between gap-4 rounded-2xl border border-border bg-[#0E2E4F] px-8 py-6 text-[#F1F7FB] sm:flex-row sm:items-center">
          <div>
            <p className="text-lg font-semibold">{enterpriseTier.name}</p>
            <p className="text-sm opacity-80">
              {enterpriseTier.description} {enterpriseTier.mcp}. Negotiated volume, unlimited
              retention, 24/7 + dedicated SE.
            </p>
          </div>
          {/* 011: map tier -> price id. Enterprise is sales-led, not self-serve checkout. */}
          <Link
            href={enterpriseTier.href}
            className={cn(
              buttonVariants({ variant: "outline" }),
              "shrink-0 !bg-[#F1F7FB] !text-[#0E2E4F] hover:!bg-white"
            )}
          >
            {enterpriseTier.buttonText}
          </Link>
        </div>
      )}
    </Section>
  );
}
