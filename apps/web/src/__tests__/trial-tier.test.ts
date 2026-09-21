import { expect, test } from "vitest";
import { canonicalTier, tierRank } from "@/lib/tier";

test("hosted trials are not mislabeled as self-host or paid plans", () => {
  expect(canonicalTier("trial")).toBe("trial");
  expect(tierRank("trial")).toBeLessThan(tierRank("indie"));
});
