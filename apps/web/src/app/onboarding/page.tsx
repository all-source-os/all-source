import { Button, Icons } from "@allsource/ui";
import type { Metadata } from "next";
import Link from "next/link";
import { Suspense } from "react";
import { OnboardingWizard } from "@/app/dashboard/demo/onboarding/page";

export const metadata: Metadata = {
  robots: { index: false, follow: false },
};

export default function OnboardingPage() {
  return (
    <div className="min-h-screen bg-background">
      <header className="border-b border-border bg-background/95">
        <div className="mx-auto flex h-16 max-w-5xl items-center justify-between px-4 sm:px-6">
          <Link href="/" className="flex items-center gap-2" aria-label="AllSource home">
            <Icons.logo className="h-7 w-7 text-primary" />
            <span className="font-semibold">AllSource</span>
          </Link>
          <Button asChild variant="ghost" size="sm">
            <Link href="/dashboard">Skip to dashboard</Link>
          </Button>
        </div>
      </header>
      <main className="px-4 py-10 sm:px-6 sm:py-14">
        <Suspense
          fallback={
            <div className="mx-auto h-96 max-w-3xl animate-pulse rounded-xl border border-border bg-card" />
          }
        >
          <OnboardingWizard basePath="/onboarding" />
        </Suspense>
      </main>
    </div>
  );
}
