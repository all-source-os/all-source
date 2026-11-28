import { historyDiscovery } from "@/lib/server/event-history-discovery";

export function GET() {
  return historyDiscovery("metadata");
}
