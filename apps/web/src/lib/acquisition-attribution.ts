export interface AcquisitionAttribution {
  campaign_content?: string;
  campaign_medium?:
    | "community"
    | "creator"
    | "newsletter"
    | "organic-social"
    | "play"
    | "portfolio";
  campaign_name?: string;
  campaign_source?: string;
}

const BET_SLUG = "allsource";
const SOURCES = new Map([
  ["wolventech.com", "wolventech.com"],
  ["decebaldobrica.com", "decebaldobrica.com"],
  ["github.com", "github"],
  ["linkedin.com", "linkedin"],
  ["x.com", "x"],
  ["reddit.com", "reddit"],
  ["youtube.com", "youtube"],
  ["substack.com", "substack"],
  ["beehiiv.com", "beehiiv"],
]);
const MEDIA = new Set([
  "community",
  "creator",
  "newsletter",
  "organic-social",
  "play",
  "portfolio",
] as const);

export function acquisitionAttributionForUrl(value: string): AcquisitionAttribution {
  try {
    const url = new URL(value);
    const rawSource = url.searchParams.get("utm_source")?.trim().toLowerCase();
    const medium = url.searchParams.get("utm_medium")?.trim().toLowerCase();
    const campaign = url.searchParams.get("utm_campaign")?.trim().toLowerCase();
    const source = rawSource ? SOURCES.get(rawSource) : undefined;
    const validCampaign =
      campaign === "product_directory" ||
      new RegExp(`^${BET_SLUG}_[0-9]{4}-[0-9]{2}$`).test(campaign ?? "");

    if (!source || !medium || !MEDIA.has(medium as never) || !validCampaign) {
      return {};
    }

    const content = url.searchParams.get("utm_content")?.trim().toLowerCase();
    return {
      ...(campaign === "product_directory" &&
      (content === `${BET_SLUG}_product` || content === `${BET_SLUG}_proof`)
        ? { campaign_content: content }
        : {}),
      campaign_medium: medium as AcquisitionAttribution["campaign_medium"],
      campaign_name: campaign,
      campaign_source: source,
    };
  } catch {
    return {};
  }
}
