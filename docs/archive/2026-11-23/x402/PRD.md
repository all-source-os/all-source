---
title: "Product Requirements Document: AllSource Paywall"
status: CURRENT
last_updated: 2026-02-02
category: project
project: x402-hackathon
---

# Product Requirements Document: AllSource Paywall

**Product:** AllSource Paywall - x402 Micropayments for Content Creators
**Version:** 1.0
**Date:** November 2025
**Owner:** [Your Name]
**Status:** Draft

---

## Executive Summary

AllSource Paywall is a SaaS platform that enables content creators to monetize their work through per-article micropayments using the x402 protocol. By leveraging AllSource event store's unique time-travel capabilities, we provide analytics and dispute resolution features impossible with traditional databases.

**One-line pitch:** "Stripe for content - add micropayments to your blog in 2 minutes."

**Market opportunity:** 50M+ bloggers worldwide, with subscription fatigue creating demand for flexible monetization alternatives.

**Competitive advantage:** Event sourcing-powered analytics with time-travel queries for granular revenue attribution and dispute resolution.

---

## 1. Problem Statement

### Current State

Content creators face significant monetization challenges:

**Problem 1: Subscriptions are all-or-nothing**
- Readers hesitate to commit to $5-10/month subscriptions
- Creators lose revenue from casual readers
- High churn rates (typical: 5-10% monthly)

**Problem 2: Complex setup**
- Stripe integration takes hours to implement
- Platforms like Patreon require managing tiers
- Moving platforms means losing payment history

**Problem 3: Poor analytics**
- Can't determine which specific articles drive revenue
- No insight into reader journey before purchase
- Opaque algorithms (Medium Partner Program)

**Problem 4: Inadequate dispute resolution**
- Hard to prove content was delivered
- Time-consuming manual investigation
- Refund decisions based on incomplete data

### Target Users

**Primary:** Independent content creators
- Writers, developers, designers
- 100-10,000 followers
- Currently on Substack, Medium, WordPress
- Earn $100-$10,000/month from content

**Secondary:** Small publications
- 2-10 writers
- Custom websites or Ghost
- Need flexible monetization options

---

## 2. Product Vision & Goals

### Vision Statement

"Enable every content creator to earn fair compensation for their work through frictionless micropayments, with unprecedented insight into what content resonates with their audience."

### Business Goals (Year 1)

**Q1 (Post-Hackathon):**
- 100 creators signed up
- $50K in payments processed
- 10 paying customers
- $300 MRR

**Q2:**
- 500 creators
- $250K processed
- 50 paying customers
- $1.5K MRR

**Q3:**
- 2,000 creators
- $1M processed
- 200 paying customers
- $6K MRR

**Q4:**
- 5,000 creators
- $3M processed
- 500 paying customers
- $15K MRR

### Product Goals

**For MVP (Hackathon):**
1. Functional paywall widget (client-side)
2. x402 payment verification
3. Basic creator dashboard
4. WordPress plugin

**For V1.0 (Month 1-2):**
1. Reliable payment flow (99% success rate)
2. Time-travel analytics (unique differentiator)
3. Multi-platform support (WordPress, Ghost)
4. Creator onboarding < 5 minutes

**For V2.0 (Month 3-6):**
1. Browser extension (Substack, Medium)
2. Reader journey analytics
3. Pricing experiment tools
4. Bundle/subscription options

---

## 3. User Personas

### Primary Persona: "Sarah the Technical Writer"

**Demographics:**
- Age: 32
- Location: San Francisco, CA
- Occupation: Senior Software Engineer, side hustle blogger
- Tech savviness: High

**Background:**
- Writes 2-3 technical tutorials per month
- Has WordPress blog with 2,000 monthly visitors
- Currently uses Google AdSense (makes $50/month)
- Wants to monetize without annoying readers

**Goals:**
- Earn $500-1,000/month from writing
- Know which topics resonate most
- Keep setup simple (< 30 minutes)
- Maintain control of her content

**Pain Points:**
- AdSense revenue is unpredictable and low
- Patreon feels too formal/committal
- Doesn't want to paywall all content
- Curious which articles people would pay for

**Quote:** "I'd love to know if people would actually pay for my advanced Kubernetes tutorial. I don't want to force subscriptions on casual readers."

---

### Secondary Persona: "Marcus the Newsletter Publisher"

**Demographics:**
- Age: 28
- Location: Austin, TX
- Occupation: Full-time content creator
- Tech savviness: Medium

**Background:**
- Runs newsletter on Substack (5,000 subscribers)
- 300 paid subscribers @ $10/month = $3,000/month
- Wants to offer pay-per-article for non-subscribers
- Frustrated by Substack's limited analytics

**Goals:**
- Increase revenue from non-subscribers
- Test different price points
- Understand reader behavior better
- Reduce churn through flexible pricing

**Pain Points:**
- Substack is all-or-nothing (free or $10/month)
- Can't charge $0.50 for single articles
- Analytics are basic (opens, clicks only)
- Platform lock-in (hard to move newsletter)

**Quote:** "I have 4,700 free subscribers who never convert. If I could charge $0.50 per premium article, I bet 10% would pay occasionally. That's $235 per article!"

---

### Tertiary Persona: "Elena the Publication Editor"

**Demographics:**
- Age: 45
- Location: New York, NY
- Occupation: Editor-in-Chief, small tech publication
- Tech savviness: Medium-low

**Background:**
- Manages 8 writers
- Custom WordPress site
- Currently has advertising + subscriptions
- Wants to experiment with article-level pricing

**Goals:**
- Diversify revenue streams
- Give writers transparent revenue share
- Understand which content types perform best
- Maintain brand consistency

**Pain Points:**
- Advertising revenue declining
- Subscription model doesn't work for occasional readers
- Hard to attribute revenue to specific writers
- Complex payment tracking with current systems

**Quote:** "We need a fair way to pay our writers based on what they produce. Current subscription model makes attribution impossible."

---

## 4. User Stories

### Epic 1: Creator Onboarding

**US-1.1:** As a creator, I want to sign up with my email and wallet address in under 2 minutes, so I can start monetizing quickly.

**Acceptance Criteria:**
- Sign up form requires: email, wallet address (Solana), blog URL
- Email verification sent within 30 seconds
- Creator receives widget script immediately after verification
- Dashboard is accessible within 5 minutes of signup

**Priority:** P0 (MVP)

---

**US-1.2:** As a creator, I want to install the paywall widget by copying a single script tag, so I don't need technical expertise.

**Acceptance Criteria:**
- Creator dashboard shows copy-pastable script tag
- Script works on any HTML page
- Script auto-detects paywall divs with data attributes
- Documentation link provided for common platforms

**Priority:** P0 (MVP)

---

**US-1.3:** As a WordPress user, I want to install a plugin that adds paywall functionality, so I don't need to edit code.

**Acceptance Criteria:**
- Plugin available in WordPress directory
- Install via WordPress admin (Plugins → Add New)
- Settings page for API key configuration
- Shortcode: `[allsource-paywall price="0.50"]content[/allsource-paywall]`
- Block editor support (Gutenberg)

**Priority:** P0 (MVP)

---

### Epic 2: Content Monetization

**US-2.1:** As a creator, I want to set different prices for different articles, so I can optimize revenue based on content value.

**Acceptance Criteria:**
- Price set via `data-price` attribute on paywall div
- Supports prices from $0.10 to $10.00
- Creator can change price without re-deploying widget
- Dashboard shows price history per article

**Priority:** P0 (MVP)

---

**US-2.2:** As a creator, I want readers to pay with cryptocurrency (USDC on Solana), so I can receive payments with low fees.

**Acceptance Criteria:**
- Widget supports Solana USDC payments
- x402 protocol compliance
- Payment verified on-chain
- Creator receives 93% of payment (7% platform fee on Creator tier)

**Priority:** P0 (MVP)

---

**US-2.3:** As a creator, I want readers to access paid content for 30 days without re-paying, so they have a good experience.

**Acceptance Criteria:**
- Access token stored in localStorage
- Token expires after 30 days
- Token tied to article ID + reader wallet
- Re-access doesn't trigger new payment

**Priority:** P1 (V1.0)

---

### Epic 3: Reader Experience

**US-3.1:** As a reader, I want to see exactly what I'm paying for before purchase, so I can make an informed decision.

**Acceptance Criteria:**
- Paywall shows: article title, price, estimated reading time
- Preview shows first paragraph or summary
- Clear "Unlock for $X" button
- Accepted payment methods displayed

**Priority:** P0 (MVP)

---

**US-3.2:** As a reader, I want to pay with my Solana wallet (Phantom, Solflare), so I can complete payment in seconds.

**Acceptance Criteria:**
- Wallet selector UI appears on payment click
- Supports: Phantom, Solflare, Backpack wallets
- Transaction completes in < 5 seconds on average
- Error messages are clear (insufficient funds, network issues)

**Priority:** P0 (MVP)

---

**US-3.3:** As a reader, I want content to unlock immediately after payment, so I don't have to wait or refresh.

**Acceptance Criteria:**
- Paywall disappears after payment confirmation
- No page reload required
- Content is readable within 2 seconds of payment
- Success message: "Payment confirmed! Enjoy your read."

**Priority:** P0 (MVP)

---

**US-3.4:** As a reader, I want to re-access paid content without re-paying, so I can read at my own pace.

**Acceptance Criteria:**
- Access granted for 30 days
- Works across devices if using same wallet
- Reader can see list of purchased articles in wallet dashboard (future)

**Priority:** P1 (V1.0)

---

### Epic 4: Creator Analytics

**US-4.1:** As a creator, I want to see total revenue by day/week/month, so I can track growth.

**Acceptance Criteria:**
- Dashboard shows: today, this week, this month, all-time revenue
- Charts show revenue over time (line graph)
- Breakdown by article (bar chart)
- Exportable as CSV

**Priority:** P0 (MVP)

---

**US-4.2:** As a creator, I want to see which articles earn the most, so I can create more of that content.

**Acceptance Criteria:**
- Table: Article title, # purchases, total revenue, avg price
- Sortable by revenue, purchases, conversion rate
- Date range filter
- Click article to see detailed analytics

**Priority:** P0 (MVP)

---

**US-4.3:** As a creator, I want to use time-travel queries to see historical reader behavior, so I can resolve disputes and understand patterns.

**Acceptance Criteria:**
- "Time Travel" button on each transaction
- Select any past timestamp
- See exact state: payment status, content access, read duration
- Use case: "Did this reader actually get access?"

**Priority:** P1 (V1.0) - Unique differentiator!

---

**US-4.4:** As a creator, I want to see reader journey analytics (which free articles led to purchases), so I can optimize my funnel.

**Acceptance Criteria:**
- "Reader Journey" view in dashboard
- Shows: Free article → Free article → Paid article (conversion path)
- Identifies common paths to purchase
- Calculates time-to-conversion (average: X days)

**Priority:** P2 (V2.0)

---

**US-4.5:** As a creator, I want to run pricing experiments (A/B test $0.50 vs $1.00), so I can maximize revenue.

**Acceptance Criteria:**
- Dashboard: "Test Price" button
- Set variant prices (A: $0.50, B: $1.00)
- Widget randomly assigns variant
- Dashboard shows: variant, conversions, revenue per variant
- Statistical significance indicator

**Priority:** P2 (V2.0)

---

### Epic 5: Platform Integrations

**US-5.1:** As a WordPress user, I want a plugin that adds paywall shortcodes, so I don't need to edit HTML.

**Acceptance Criteria:**
- WordPress plugin with < 5-minute setup
- Shortcode: `[allsource-paywall price="0.50"]content[/allsource-paywall]`
- Gutenberg block: "AllSource Paywall"
- Settings page: API key, default price, wallet address

**Priority:** P0 (MVP)

---

**US-5.2:** As a Ghost user, I want clear documentation for adding the widget, so I can integrate without developer help.

**Acceptance Criteria:**
- Ghost integration guide in docs
- Copy-paste code injection instructions
- Video tutorial (< 3 minutes)
- Example post with paywall

**Priority:** P1 (V1.0)

---

**US-5.3:** As a Substack creator, I want a browser extension that adds paywalls to my posts, so I can offer per-article pricing.

**Acceptance Criteria:**
- Chrome extension available in Web Store
- Firefox add-on available
- Detects special marker: `🔒 ALLSOURCE-PAYWALL:0.50`
- Injects paywall widget automatically
- Reader sees native-looking paywall

**Priority:** P2 (V2.0)

---

### Epic 6: Payment Management

**US-6.1:** As a creator, I want to withdraw my earnings to my Solana wallet, so I can access my money.

**Acceptance Criteria:**
- "Withdraw" button in dashboard
- Minimum withdrawal: $10
- Funds sent within 24 hours
- Email confirmation of withdrawal
- Transaction hash provided

**Priority:** P1 (V1.0)

---

**US-6.2:** As a creator, I want to see a detailed transaction history, so I can reconcile my earnings.

**Acceptance Criteria:**
- Table: Date, article, reader (anonymized), amount, status
- Filter by: date range, article, status
- Export as CSV
- Links to on-chain transaction

**Priority:** P1 (V1.0)

---

**US-6.3:** As a creator, I want to handle disputes with time-travel proof, so I can resolve issues quickly.

**Acceptance Criteria:**
- "Dispute" flag on transactions
- Time-travel to exact moment of payment
- Export dispute proof (PDF with timestamps)
- Refund button (if needed)

**Priority:** P1 (V1.0) - Unique feature!

---

### Epic 7: Reader Management

**US-7.1:** As a creator, I want to see anonymized reader behavior (purchases, reading time), so I can understand my audience.

**Acceptance Criteria:**
- Reader identified by wallet address (anonymized: "0x1234...5678")
- Shows: # articles purchased, total spent, favorite topics
- Respects privacy (no PII collected)

**Priority:** P2 (V2.0)

---

**US-7.2:** As a creator, I want to offer bundles ("Buy 5 articles for $2"), so I can increase average transaction value.

**Acceptance Criteria:**
- Dashboard: "Create Bundle" button
- Set: # articles, price, expiration
- Widget shows bundle offer
- Reader can apply bundle to any articles

**Priority:** P2 (V2.0)

---

---

## 5. Feature Requirements

### 5.1 MVP (Hackathon - Week 0)

**Must Have:**
- [ ] Paywall widget (vanilla JS)
  - Detects `data-allsource-paywall` divs
  - Injects "Unlock for $X" overlay
  - Handles x402 payment flow
  - Unlocks content after verification
- [ ] Backend API
  - POST `/v1/content/payment-verify` - Verify x402 payment
  - POST `/v1/content/unlock` - Issue access token
  - GET `/v1/creator/analytics` - Basic revenue data
- [ ] Creator dashboard
  - Sign up / login
  - Widget installation instructions
  - Total revenue (today, week, month)
  - Revenue by article (table)
- [ ] WordPress plugin (basic)
  - Shortcode support
  - API key configuration
- [ ] AllSource integration
  - Event logging: `paywall.article.purchased`
  - Event logging: `paywall.content.accessed`
  - Query API for analytics

**Success Criteria:**
- Complete payment flow works end-to-end
- Creator can see revenue in dashboard
- 10 beta testers successfully install

---

### 5.2 V1.0 (Month 1-2)

**Must Have:**
- [ ] Enhanced widget
  - Multi-wallet support (Phantom, Solflare, Backpack)
  - Error handling & retry logic
  - Loading states & animations
  - Mobile-responsive
- [ ] Time-travel analytics ⭐
  - "Time Travel" button on each transaction
  - Reconstruct payment state at any timestamp
  - Dispute resolution proof export
- [ ] Improved dashboard
  - Charts (revenue over time, by article)
  - Date range filters
  - CSV export
- [ ] WordPress plugin (enhanced)
  - Gutenberg block editor
  - Preview mode
  - Analytics in WordPress admin
- [ ] Ghost integration
  - Documentation
  - Code injection guide
  - Example theme customization
- [ ] Payment management
  - Withdrawal to Solana wallet
  - Transaction history
  - Email notifications

**Nice to Have:**
- [ ] Reader dashboard (view purchased articles)
- [ ] Bundle pricing (basic)
- [ ] Multiple blockchain support (Base)

**Success Criteria:**
- 100 active creators
- $50K processed
- 95% payment success rate
- < 1% dispute rate

---

### 5.3 V2.0 (Month 3-6)

**Must Have:**
- [ ] Browser extension
  - Chrome Web Store
  - Firefox Add-ons
  - Detect Substack/Medium posts with marker
  - Inject paywall seamlessly
- [ ] Reader journey analytics
  - Track: Free article → Paid article conversions
  - Identify top conversion paths
  - Calculate time-to-conversion
- [ ] Pricing experiments
  - A/B test different price points
  - Statistical significance calculator
  - Automatic winner selection
- [ ] API access (Pro tier)
  - RESTful API for custom integrations
  - Webhooks for payment events
  - SDK (TypeScript, Python)

**Nice to Have:**
- [ ] Subscription option (hybrid model)
- [ ] Email capture for marketing
- [ ] Creator community features
- [ ] White-label option (Enterprise)

**Success Criteria:**
- 500 active creators
- $250K processed
- Browser extension: 1,000+ installs
- 50 API integrations

---

## 6. Technical Requirements

### 6.1 Architecture

**Frontend:**
- Paywall widget: Vanilla JS (compatibility)
- Creator dashboard: Next.js 15 + React 19
- Styling: Tailwind CSS

**Backend:**
- API: Node.js + Express (or Go for performance)
- Authentication: JWT
- Rate limiting: Redis
- File storage: S3 (for dispute proofs)

**Database:**
- Primary: AllSource Event Store (existing)
- Cache: Redis
- Optional: PostgreSQL (for creator accounts)

**Payment:**
- x402 protocol compliance
- Solana Web3.js
- Multi-chain support (future): Base, Polygon

**Infrastructure:**
- Hosting: Vercel (frontend), Fly.io (backend)
- CDN: CloudFlare (widget delivery)
- Monitoring: Sentry (errors), Plausible (analytics)

---

### 6.2 Event Schema (AllSource)

**Core Events:**

```typescript
interface PaywallArticlePurchased {
  event_type: 'paywall.article.purchased';
  entity_id: string; // article-id
  aggregate_id: string; // transaction-id
  timestamp: number;
  data: {
    transaction_id: string;
    article_id: string;
    article_title: string;
    article_url: string;
    creator_id: string;
    reader_wallet: string; // anonymized
    amount_usd: number;
    amount_crypto: number;
    currency: 'USDC' | 'SOL';
    blockchain: 'solana' | 'base' | 'polygon';
    tx_signature: string;
    price_point: number; // for A/B testing
    conversion_path?: string[]; // free articles visited before purchase
  };
}

interface PaywallContentAccessed {
  event_type: 'paywall.content.accessed';
  entity_id: string; // reader-wallet
  aggregate_id: string; // session-id
  timestamp: number;
  data: {
    article_id: string;
    creator_id: string;
    reader_wallet: string;
    access_method: 'paid' | 'free' | 'bundle';
    read_duration_seconds: number;
    scroll_depth_percent: number;
    device_type: 'mobile' | 'desktop' | 'tablet';
  };
}

interface PaywallDisputeFiled {
  event_type: 'paywall.dispute.filed';
  entity_id: string; // dispute-id
  aggregate_id: string; // transaction-id
  timestamp: number;
  data: {
    dispute_id: string;
    transaction_id: string;
    reader_wallet: string;
    creator_id: string;
    article_id: string;
    reason: string;
    status: 'pending' | 'resolved' | 'refunded';
  };
}

interface PaywallPricingExperiment {
  event_type: 'paywall.pricing.experiment';
  entity_id: string; // experiment-id
  aggregate_id: string; // article-id
  timestamp: number;
  data: {
    experiment_id: string;
    article_id: string;
    variant: 'A' | 'B';
    price_shown: number;
    converted: boolean;
  };
}
```

---

### 6.3 API Endpoints

**Public API (Widget):**

```typescript
POST /v1/content/check-access
Body: { article_id, reader_wallet }
Response: { has_access: boolean, expires_at?: number }

POST /v1/content/payment-verify
Body: { article_id, tx_signature, reader_wallet }
Response: { access_token: string, expires_at: number }

GET /v1/content/unlock/:article_id
Headers: { Authorization: Bearer <access_token> }
Response: { content: string } // Or redirect to original content
```

**Creator API (Dashboard):**

```typescript
GET /v1/creator/analytics/revenue
Query: { start_date, end_date, granularity: 'day'|'week'|'month' }
Response: { total: number, by_period: Array<{date, amount}> }

GET /v1/creator/analytics/articles
Query: { start_date, end_date, sort_by: 'revenue'|'purchases' }
Response: { articles: Array<{id, title, purchases, revenue}> }

GET /v1/creator/analytics/time-travel
Query: { transaction_id, timestamp }
Response: { state: PaymentState, proof: DisputeProof }

POST /v1/creator/withdrawal
Body: { amount, wallet_address }
Response: { withdrawal_id, status, tx_signature }

GET /v1/creator/transactions
Query: { start_date, end_date, article_id?, status? }
Response: { transactions: Array<Transaction> }
```

**Admin API (Internal):**

```typescript
POST /admin/creators
Body: { email, wallet, blog_url }
Response: { creator_id, api_key }

GET /admin/stats
Response: { total_creators, total_volume, active_creators }

POST /admin/disputes/resolve
Body: { dispute_id, resolution: 'refund'|'deny', reason }
Response: { status: 'resolved' }
```

---

### 6.4 Widget Integration

**Installation (HTML):**

```html
<!-- Step 1: Add widget script to <head> -->
<script src="https://paywall.all-source.xyz/widget.js"
        data-creator-id="your-creator-id"
        data-network="solana"></script>

<!-- Step 2: Wrap premium content -->
<div data-allsource-paywall
     data-price="0.50"
     data-article-id="my-article-slug"
     data-title="How to Scale to 1M Users">

  <h2>Premium Content</h2>
  <p>This content requires payment...</p>

</div>
```

**Installation (WordPress):**

```php
// In post editor
[allsource-paywall price="0.50" article-id="my-article"]
Premium content here...
[/allsource-paywall]

// Or use Gutenberg block
```

**Installation (Ghost):**

```html
<!-- Settings > Code Injection > Site Header -->
<script src="https://paywall.all-source.xyz/widget.js"
        data-creator-id="{{@site.uuid}}"></script>

<!-- In your post -->
<div data-allsource-paywall data-price="0.50">
  Premium content...
</div>
```

---

### 6.5 Security Requirements

**Payment Security:**
- [ ] Verify Solana transaction on-chain (can't be spoofed)
- [ ] Check transaction recipient matches creator wallet
- [ ] Validate transaction amount meets minimum price
- [ ] Prevent replay attacks (transaction used only once)
- [ ] Rate limit payment verification (prevent abuse)

**Access Token Security:**
- [ ] JWT tokens signed with secret key
- [ ] Tokens expire after 30 days
- [ ] Tokens tied to article ID + reader wallet
- [ ] Invalidate tokens on dispute/refund

**Creator Account Security:**
- [ ] Email verification required
- [ ] API keys rotatable
- [ ] Two-factor authentication (Pro tier)
- [ ] Audit log of account changes

**Widget Security:**
- [ ] Served over HTTPS/CDN
- [ ] CSP (Content Security Policy) compatible
- [ ] No eval() or dangerous DOM manipulation
- [ ] XSS protection

---

### 6.6 Performance Requirements

**Widget Performance:**
- Load time: < 200ms (50kb gzipped)
- No blocking of page render
- Lazy load wallet connectors
- Cache access tokens locally

**API Performance:**
- Payment verification: < 2 seconds (p95)
- Analytics queries: < 500ms (p95)
- Time-travel queries: < 1 second (p95)
- Dashboard load: < 1 second (p95)

**Reliability:**
- Uptime: 99.9% (V1.0), 99.99% (V2.0)
- Payment success rate: > 95%
- Data durability: 99.999999999% (AllSource)

**Scalability:**
- Support 10,000 creators (V1.0)
- Process 100,000 payments/month (V1.0)
- Handle 1M API requests/day (V2.0)

---

## 7. UX Requirements

### 7.1 Widget UX (Reader-facing)

**Paywall Overlay:**
- Clear pricing: "Unlock for $0.50"
- Article preview (first paragraph)
- Estimated reading time
- Accepted payment methods (wallet icons)
- "Why pay?" explanation

**Payment Flow:**
- Click "Unlock" → Wallet selector appears
- Select wallet (Phantom, Solflare, Backpack)
- Approve transaction in wallet
- Loading state: "Verifying payment..."
- Success: "Payment confirmed! Enjoy your read."
- Content unlocks without page refresh

**Error Handling:**
- Insufficient funds: "Add USDC to your wallet"
- Transaction failed: "Try again or contact support"
- Network issues: "Check your connection"
- Timeout: "Payment verification taking longer than usual"

**Mobile Experience:**
- Touch-friendly buttons (min 44px)
- Wallet deep-linking (Phantom app)
- Responsive layout
- Fast payment flow (< 10 seconds)

---

### 7.2 Dashboard UX (Creator-facing)

**Onboarding:**
1. Sign up form (email + wallet)
2. Email verification
3. Welcome screen: "Here's your widget code"
4. Quick setup guide (< 5 minutes)
5. Test payment with dummy article

**Dashboard Home:**
- Hero metrics: Today's revenue, this week, this month
- Revenue chart (line graph, last 30 days)
- Top 5 articles by revenue (bar chart)
- Recent transactions (table, last 10)
- Quick actions: "Add new article", "Withdraw funds"

**Analytics Page:**
- Date range picker
- Revenue breakdown (by article, by day)
- Conversion metrics (visitors → purchases)
- Time-travel query builder (advanced)

**Settings Page:**
- API key (show/hide, rotate)
- Wallet address (change with verification)
- Email preferences (notifications)
- Account deletion

**Visual Design:**
- Clean, minimal interface
- Dark mode support
- Loading skeletons (not spinners)
- Consistent iconography
- Accessible (WCAG 2.1 AA)

---

## 8. Non-Functional Requirements

### 8.1 Compliance & Legal

**Data Privacy:**
- GDPR compliant (EU users)
- CCPA compliant (California users)
- No PII collected without consent
- Data retention: 2 years (or as required)
- Right to deletion (account deletion removes data)

**Financial Compliance:**
- Not a money transmitter (crypto-to-crypto)
- 1099 reporting for creators (if > $600/year)
- Anti-money laundering (basic checks)
- Fraud prevention (rate limiting, anomaly detection)

**Terms of Service:**
- Creator agreement (revenue share, fees)
- Reader agreement (refund policy)
- Prohibited content (illegal, copyrighted)
- Dispute resolution process

---

### 8.2 Monitoring & Observability

**Metrics to Track:**
- Payment success rate (target: > 95%)
- Average payment time (target: < 5 seconds)
- Dashboard load time (target: < 1 second)
- API error rate (target: < 1%)
- Creator churn rate (target: < 5%/month)

**Alerts:**
- Payment success rate drops below 90%
- API error rate exceeds 5%
- AllSource event store unavailable
- Creator withdrawal failures

**Logging:**
- All payment attempts (success + failures)
- API requests (rate limiting, errors)
- Creator actions (login, withdrawal, settings)
- Widget loads (for debugging)

---

### 8.3 Testing Requirements

**Unit Tests:**
- Payment verification logic
- Access token generation/validation
- x402 protocol compliance
- Event logging to AllSource

**Integration Tests:**
- End-to-end payment flow
- Dashboard → API → AllSource
- Widget → API → Blockchain
- WordPress plugin activation

**E2E Tests (Playwright):**
- Reader: View article → Pay → Unlock
- Creator: Sign up → Install widget → See revenue
- Dispute: File dispute → Time-travel → Resolve

**Load Tests:**
- 100 concurrent payments
- 1,000 analytics queries/minute
- 10,000 widget loads/minute

---

## 9. Dependencies & Integrations

### 9.1 Internal Dependencies

**AllSource Event Store:**
- Must be deployed and accessible
- Ingestion API: `/api/v1/events`
- Query API: `/api/v1/events/query`
- WebSocket: `/api/v1/events/stream`
- Time-travel API: `/api/v1/entities/:id/state?as_of=`

**x402 SDK:**
- Built during hackathon
- Handles payment verification
- Logs events to AllSource

---

### 9.2 External Dependencies

**Blockchain:**
- Solana RPC (Helius, QuickNode, or public)
- Solana Web3.js library
- Wallet adapters (Phantom, Solflare, Backpack)

**Infrastructure:**
- Vercel (hosting)
- Fly.io (backend API)
- CloudFlare (CDN for widget)
- AWS S3 (dispute proofs)

**Third-party Services:**
- Stripe (for creator subscriptions, not reader payments)
- SendGrid (email notifications)
- Sentry (error tracking)
- Plausible (privacy-friendly analytics)

---

### 9.3 Platform Integrations

**WordPress:**
- Plugin development (PHP)
- WordPress.org directory submission
- Gutenberg block registration
- Settings API integration

**Ghost:**
- Code injection documentation
- Theme customization guide
- Ghost Admin API (optional)

**Substack/Medium (Future):**
- Browser extension (Chrome, Firefox)
- Content script injection
- Marker detection algorithm

---

## 10. Timeline & Milestones

### Phase 0: Hackathon (Week 0)

**Goal:** Prove technical feasibility + win hackathon

**Deliverables:**
- [ ] Widget MVP (paywall overlay, payment flow)
- [ ] Backend API (verify, unlock)
- [ ] Dashboard MVP (revenue, articles)
- [ ] WordPress plugin (basic)
- [ ] Demo video (2 minutes)
- [ ] 10 beta testers signed up

**Success Criteria:**
- Complete payment flow works
- Hackathon submission accepted
- Positive feedback from judges

---

### Phase 1: Validation (Week 1-4)

**Goal:** Validate demand, refine MVP

**Week 1-2:**
- [ ] Launch landing page (waitlist)
- [ ] Collect 100 email signups
- [ ] Interview 10 creators
- [ ] Refine pricing based on feedback

**Week 3-4:**
- [ ] Onboard 10 beta creators
- [ ] Process $1,000 in payments
- [ ] Iterate based on feedback
- [ ] Fix critical bugs
- [ ] Document setup process

**Success Criteria:**
- 10 active creators
- $1K processed
- 95% payment success rate
- Positive creator feedback (NPS > 40)

---

### Phase 2: Public Launch (Month 2-3)

**Goal:** Grow to 100 creators, $50K processed

**Month 2:**
- [ ] Polish widget UI
- [ ] Enhance dashboard (charts, export)
- [ ] WordPress plugin → directory
- [ ] Ghost integration docs
- [ ] ProductHunt launch
- [ ] Content marketing (blog, Twitter)

**Month 3:**
- [ ] Time-travel analytics (V1.0)
- [ ] Payment management (withdrawals)
- [ ] Email notifications
- [ ] Customer support system
- [ ] Stripe integration (for creator subscriptions)

**Success Criteria:**
- 100 active creators
- $50K processed
- 10 paying customers ($29/month)
- WordPress plugin: 100+ installs

---

### Phase 3: Platform Expansion (Month 4-6)

**Goal:** Grow to 500 creators, $250K processed

**Month 4:**
- [ ] Browser extension (Chrome)
- [ ] Reader journey analytics
- [ ] Pricing experiments (A/B testing)

**Month 5:**
- [ ] Firefox extension
- [ ] Bundle pricing
- [ ] API access (Pro tier)
- [ ] Webhooks

**Month 6:**
- [ ] Multi-chain support (Base)
- [ ] White-label option (Enterprise)
- [ ] Partner outreach (Ghost, Medium)

**Success Criteria:**
- 500 active creators
- $250K processed
- 50 paying customers
- Browser extension: 1,000+ installs

---

## 11. Risks & Mitigations

### Risk 1: Low Creator Adoption

**Risk:** Creators don't see value in micropayments vs subscriptions.

**Likelihood:** Medium
**Impact:** High

**Mitigation:**
- Offer free tier (test before committing)
- Showcase successful case studies
- Provide comparison calculator (micropayments vs subscriptions)
- Target crypto-native creators first (understand x402)

---

### Risk 2: Reader Friction (Payment UX)

**Risk:** Readers find crypto payments too complex.

**Likelihood:** High
**Impact:** High

**Mitigation:**
- Wallet selector with clear instructions
- Support popular wallets (Phantom most common)
- "What is USDC?" educational content
- Consider credit card on-ramp (future)

---

### Risk 3: Blockchain Network Issues

**Risk:** Solana downtime or congestion impacts payments.

**Likelihood:** Medium
**Impact:** Medium

**Mitigation:**
- Multi-chain support (Solana + Base)
- Fallback RPC endpoints
- Queue failed payments for retry
- Status page (transparency)

---

### Risk 4: Regulatory Uncertainty

**Risk:** x402 or crypto payments face regulatory challenges.

**Likelihood:** Low-Medium
**Impact:** High

**Mitigation:**
- Legal counsel consultation
- Monitor regulatory developments
- Avoid money transmission (crypto-to-crypto only)
- Geographic restrictions if needed

---

### Risk 5: Competition

**Risk:** Larger players (Stripe, Substack) add similar features.

**Likelihood:** Medium
**Impact:** Medium

**Mitigation:**
- Move fast (first-mover advantage)
- Differentiate with event sourcing analytics (time-travel)
- Build creator community (lock-in via network effects)
- Focus on crypto-native use cases (they won't)

---

### Risk 6: Technical: AllSource Scalability

**Risk:** AllSource event store can't handle scale.

**Likelihood:** Low
**Impact:** High

**Mitigation:**
- Load testing before launch
- Horizontal scaling plan
- Caching layer (Redis)
- Database read replicas
- Monitor performance metrics

---

## 12. Open Questions

**Product:**
- [ ] Should we support credit card payments (via on-ramp)?
- [ ] What's the optimal price range ($0.10-$10 or allow higher)?
- [ ] Should we offer subscriptions alongside micropayments?
- [ ] How do we handle refunds (policy, process)?

**Technical:**
- [ ] Go or Node.js for backend? (Go = performance, Node = speed)
- [ ] Hosted AllSource or self-hosted option?
- [ ] Real-time analytics (WebSocket) or polling?
- [ ] Content encryption (optional for creators)?

**Business:**
- [ ] Platform fee: 7% or 5%? (compare to Substack's 10%)
- [ ] Free tier limitations (revenue cap or feature cap)?
- [ ] Enterprise pricing model (% or flat fee)?
- [ ] Partnership revenue share (Ghost, WordPress)?

**Legal:**
- [ ] Do we need money transmitter licenses?
- [ ] GDPR compliance for reader data?
- [ ] Terms of service for prohibited content?
- [ ] Dispute resolution arbitration clause?

---

## 13. Success Metrics (KPIs)

### North Star Metric

**Total Payment Volume Processed**
Why: Directly reflects creator and reader value. Grows with both adoption and engagement.

**Target:** $3M processed in Year 1

---

### Product Metrics

**Creator Metrics:**
- Active creators (monthly): 5,000 (Year 1)
- Creator retention (90-day): 70%
- Average revenue per creator: $600/month
- Time to first payment: < 7 days

**Reader Metrics:**
- Payment success rate: > 95%
- Average payment time: < 5 seconds
- Re-access rate (30-day): 40%
- Conversion rate (visitor → payer): 3-5%

**Technical Metrics:**
- API uptime: 99.9%
- Dashboard load time: < 1 second
- Payment verification latency: < 2 seconds (p95)

---

### Business Metrics

**Revenue:**
- MRR (Monthly Recurring Revenue): $15K (Year 1)
- Platform fees (7% of volume): $210K annualized
- Total ARR: $384K (Year 1)

**Growth:**
- Month-over-month creator growth: 25%
- Churn rate: < 5%/month
- Customer acquisition cost (CAC): < $50
- Lifetime value (LTV): > $500

---

## 14. Appendix

### 14.1 Glossary

**x402:** HTTP-based payment protocol enabling micropayments over the internet.

**AllSource:** Event store platform with time-travel query capabilities.

**Event Sourcing:** Architecture pattern where state changes are stored as immutable events.

**Time-Travel Query:** Ability to reconstruct system state at any past timestamp.

**Paywall:** UI element requiring payment before content access.

**Widget:** Client-side JavaScript component embedded in creator's website.

**Creator:** Content producer monetizing with AllSource Paywall.

**Reader:** Content consumer making micropayments.

**USDC:** USD Coin, stablecoin used for payments.

**Solana:** High-performance blockchain supporting fast, low-cost transactions.

---

### 14.2 References

**x402 Protocol:**
- GitHub: https://github.com/coinbase/x402
- Whitepaper: https://x402.org/x402-whitepaper.pdf
- Docs: https://docs.cdp.coinbase.com/x402/welcome

**AllSource Event Store:**
- Repository: /services/core/
- API Documentation: /services/core/src/api.rs
- Event Schema: /services/core/src/domain/entities/

**Competitor Analysis:**
- Substack: https://substack.com
- Medium Partner Program: https://medium.com/creators
- Patreon: https://patreon.com
- Ghost: https://ghost.org

---

### 14.3 Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 0.1 | 2025-11-04 | [Name] | Initial draft |
| 1.0 | 2025-11-04 | [Name] | Complete PRD for review |

---

**Next Steps:**
1. Review PRD with team
2. Get stakeholder approval
3. Begin MVP development (hackathon)
4. Schedule weekly check-ins during Phase 1

---

**Contact:** [Your Email]
**Last Updated:** November 4, 2025
**Status:** Draft - Awaiting Review
