import Link from "next/link";
import { ChangelogUnsubscribe } from "@/components/changelog-unsubscribe";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Unsubscribe from the changelog",
  description: "Stop receiving AllSource changelog and benchmark emails.",
  canonical: "/changelog/unsubscribe",
  robots: { index: false, follow: false },
});

interface UnsubscribePageProps {
  searchParams: Promise<{ id?: string; token?: string }>;
}

export default async function ChangelogUnsubscribePage({ searchParams }: UnsubscribePageProps) {
  const { id = "", token = "" } = await searchParams;
  const valid = /^[a-f0-9]{32}$/.test(id) && /^[a-f0-9]{64}$/.test(token);

  return (
    <div className="mx-auto w-full max-w-screen-sm px-4 lg:px-8 py-24 space-y-6">
      <h1 className="text-3xl font-bold text-foreground">Unsubscribe from the changelog</h1>
      {valid ? (
        <>
          <p className="text-muted-foreground leading-relaxed">
            Stop receiving AllSource changelog and benchmark emails at this address.
          </p>
          <ChangelogUnsubscribe subscriberId={id} token={token} />
        </>
      ) : (
        <p className="text-muted-foreground leading-relaxed">
          This link is incomplete. Use the unsubscribe link at the bottom of any changelog email, or
          write to the address in our{" "}
          <Link href="/privacy#changelog-emails" className="underline underline-offset-4">
            privacy policy
          </Link>
          .
        </p>
      )}
    </div>
  );
}
