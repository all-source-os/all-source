import Link from "next/link";
import { constructMetadata } from "@/lib/utils";
import { DesignPartnerForm } from "./design-partner-form";

export const metadata = constructMetadata({
  title: "AI Agent Memory Design Partner Program",
  description:
    "Five teams. Sixty hosted days. Founder-led integration for AI agents with one reproducible memory failure.",
  canonical: "/design-partners",
  image: "/design-partners/opengraph-image",
  imageAlt: "AllSource design partner program for durable AI agent memory",
});

type SearchValue = string | string[] | undefined;

interface DesignPartnerPageProps {
  searchParams: Promise<Record<string, SearchValue>>;
}

function first(value: SearchValue): string {
  return Array.isArray(value) ? value[0] || "" : value || "";
}

export default async function DesignPartnerPage({ searchParams }: DesignPartnerPageProps) {
  const params = await searchParams;
  const campaignSource = {
    source: first(params.utm_source),
    medium: first(params.utm_medium),
    campaign: first(params.utm_campaign),
    content: first(params.utm_content),
    term: first(params.utm_term),
  };

  return (
    <div className="bg-[#0B1522] text-white">
      <section className="mx-auto w-full max-w-3xl px-4 py-8 sm:px-6 sm:py-10">
        <header>
          <p className="border-l-2 border-[#33C6D0] pl-3 text-sm font-semibold text-[#72E7E2]">
            Design partner program <span className="text-slate-300">· Five teams</span>
          </p>
          <h1 className="mt-4 max-w-[14ch] text-4xl font-semibold leading-[1.04] tracking-[-0.035em] text-white sm:text-5xl">
            Fix one memory failure with us.
          </h1>
          <p className="mt-4 max-w-2xl text-lg leading-8 text-slate-300">
            Tell us where your agent loses context, source links, or past decisions. We&apos;ll help
            one working flow survive restarts and reconstruct what happened.
          </p>

          <div className="mt-6 border-y border-white/15 py-4 text-sm leading-6 text-slate-300 sm:grid sm:grid-cols-2 sm:gap-8">
            <p>
              <strong className="font-semibold text-white">You get:</strong> 60 days of Scale, one
              founder session, and two follow-ups.
            </p>
            <p className="mt-2 sm:mt-0">
              <strong className="font-semibold text-white">Good fit:</strong> one reproducible
              memory failure and time to integrate within 60 days.
            </p>
          </div>
        </header>

        <div className="mt-6">
          <DesignPartnerForm campaignSource={campaignSource} />
        </div>

        <div className="mt-6 flex flex-col gap-3 border-t border-white/15 pt-5 text-sm leading-6 text-slate-400 sm:flex-row sm:items-center sm:justify-between">
          <p>Reply within five business days. No review or public mention required.</p>
          <div className="flex gap-5">
            <Link
              href="/docs/prime"
              className="text-slate-200 underline decoration-white/30 underline-offset-4 focus-visible:rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#33C6D0]"
            >
              Technical docs
            </Link>
            <Link
              href="/privacy#design-partner-applications"
              className="text-slate-200 underline decoration-white/30 underline-offset-4 focus-visible:rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#33C6D0]"
            >
              Application privacy
            </Link>
          </div>
        </div>
      </section>
    </div>
  );
}
