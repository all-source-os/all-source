# Analytics accuracy

## Google Analytics 4

- Account: Wolven Tech Founder Mode (`406015586`)
- Property: AllSource (`551578074`)
- Web stream: AllSource Public Website (`15500514397`)
- Measurement ID: `G-3347JLG51K`
- Production origin: `https://www.all-source.xyz`
- Reporting: United Kingdom, GBP

Implementation loads `gtag.js` lazily and sends sanitized page views without URL or referrer query
strings. Consent defaults deny analytics storage, ad storage, ad user data, and ad personalisation.
Google Signals and ad-personalisation signals stay disabled.

Enhanced Measurement remains enabled for aggregate scroll, outbound-link, form, video, and download
events. Browser-history page views are disabled because Next.js route changes are measured manually;
this prevents duplicate page views. Site-search capture is disabled to prevent search query values
from reaching Google Analytics.

## Verification checklist

- [x] GA4 property and web stream created
- [x] Measurement ID configured with a canonical-production-host fallback and environment override
- [x] TypeScript checks, lint, tests, and production build pass
- [x] Production Google tag detected after deployment
- [x] Tag Assistant shows a `Page View` hit for `G-3347JLG51K`
- [ ] GA4 Data API returns production traffic

GA4 Data API authentication and property access pass without error. Production traffic remains
unknown until rows aggregate; current empty results are not recorded as zero.

Unknown data must remain unknown, not zero. GA4 has no AllSource history before collector deployment.

## Organic source contract — 30 August 2026

GA4 page views keep path-only `page_location` and now receive allow-listed
manual-campaign fields for portfolio, community, creator, newsletter, social,
and Play acquisition. Campaign must be `product_directory` or
`allsource_YYYY-MM`; arbitrary UTM values and query strings are dropped. Source
tests pass; production deployment and receipt remain pending.
