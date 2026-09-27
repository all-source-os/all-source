import { connection } from "next/server";
import { CustomerConnections } from "@/components/settings/customer-connections";

export default async function ConnectionsPage() {
  await connection();
  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <div>
        <h1 className="text-3xl font-semibold tracking-tight">Agent connections</h1>
        <p className="mt-3 text-base leading-relaxed text-muted-foreground">
          Choose what your assistant can read. Review decisions stay in your workspace.
        </p>
      </div>
      {process.env.CUSTOMER_CONNECTIONS_ENABLED === "true" ? (
        <CustomerConnections />
      ) : (
        <p className="rounded-xl border p-6 text-base">Agent connections are not available yet.</p>
      )}
    </div>
  );
}
