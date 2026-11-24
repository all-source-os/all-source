# Action Plan

## Priority Queue

| Severity | Issue |
|----------|-------|
| Critical | Performance: INP is above target at 390ms. |
| Critical | Performance: LCP is above target at 4.94s. |
| Critical | Performance: Real-user/PageSpeed performance data was unavailable, so the report uses deterministic lab heuristics. |
| High | Schema: Recommended schema type(s) are missing: WebPage. |
| High | Technical: IndexNow support was not detected. |
| Medium | Geo: No strong 134-167 word self-contained answer block was detected. |
| Medium | Images: 1 below-the-fold sampled image(s) are not lazy loaded. |
| Medium | On_Page: 23 page(s) have title tags longer than 60 characters. |
| Medium | On_Page: 50 page(s) have meta descriptions longer than 160 characters. |
| Medium | Schema: FAQPage is present on a non-government/non-healthcare page and should not be positioned as a Google rich-result tactic. |

## Recommended Actions

- **Technical**: Prioritize the hero/LCP element, reduce render-blocking resources, and compress above-the-fold assets.
- **Technical**: Reduce main-thread JavaScript work and defer non-critical third-party scripts.
- **Technical**: Consider IndexNow if faster Bing/Yandex discovery matters to the publishing workflow.
- **Performance**: Prioritize the hero/LCP element, reduce render-blocking resources, and compress above-the-fold assets.
- **Performance**: Reduce main-thread JavaScript work and defer non-critical third-party scripts.
- **Performance**: Provide `PAGESPEED_API_KEY` or re-run in an environment with PageSpeed API access for richer CWV evidence.
- **On Page**: Shorten long title tags to 50-60 characters for optimal SERP display.
- **On Page**: Trim meta descriptions to 150-160 characters to avoid truncation.
- **Schema**: Add WebPage markup aligned with the current page intent.
- **Images**: Use native `loading="lazy"` on below-the-fold images only.
- **Geo**: Add one or more self-contained 134-167 word answer blocks near key H2 sections.
- **Sitemap**: Keep the sitemap focused on canonical 200-status URLs and refresh it when key pages change.
