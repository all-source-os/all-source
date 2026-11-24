---
title: "AllSource x402 Hackathon - Battle Plan"
status: CURRENT
last_updated: 2026-02-02
category: project
project: x402-hackathon
---

# 🎯 AllSource x402 Hackathon - Battle Plan

**Your complete strategy and reference guide for winning the hackathon**

---

## 📦 What's Been Prepared For You

### Documentation Created
1. ✅ **HACKATHON_CHECKLIST.md** - Complete step-by-step checklist (main document)
2. ✅ **QUICK_REFERENCE.md** - One-page command reference (print this!)
3. ✅ **PROGRESS_TRACKER.md** - Live tracking document (update regularly)
4. ✅ **README.md** - Overview and documentation index
5. ✅ **scripts/x402-setup.sh** - Automated environment verification
6. ✅ **.env.example** - Updated with x402 variables

### Project Structure Ready
```
allsource-monorepo/
├── packages/
│   └── x402-solana-sdk/          # TO BUILD: Core SDK
├── apps/
│   ├── x402-demo/                # TO BUILD: Demo AI API
│   └── x402-dashboard/           # TO BUILD: Analytics dashboard
├── services/
│   └── core/                     # EXISTING: AllSource (just deploy)
├── docs/x402/                    # COMPLETE: All planning docs
└── scripts/x402-setup.sh         # COMPLETE: Setup verification
```

---

## 🎯 Core Insight: Leverage What Exists

### Already Built (70% Done!) ✅
- ✅ Event store engine (469K events/sec)
- ✅ Event ingestion API (`POST /api/v1/events`)
- ✅ Query API (`GET /api/v1/events/query`)
- ✅ WebSocket streaming (`/api/v1/events/stream`)
- ✅ Time-travel queries (`/api/v1/entities/:id/state?as_of=timestamp`)
- ✅ Multi-tenant support
- ✅ Audit logging
- ✅ Next.js app infrastructure
- ✅ UI component library

### Need to Build (30% New) 🔨
- 🔨 x402 TypeScript SDK (payment wrapper)
- 🔨 Solana transaction verification
- 🔨 x402 middleware function
- 🔨 Demo AI API endpoint
- 🔨 Dashboard payment feed UI
- 🔨 Time-travel debugger component

---

## 🎯 The Winning Strategy

### Your Competitive Advantage: Time-Travel Debugging 🕰️

**What it is:**
Reconstruct the EXACT state of any payment at any point in time.

**Why it matters:**
- Impossible with PostgreSQL, MongoDB, or Redis
- Only event sourcing enables this
- Perfect for disputes, audits, compliance

**Demo impact:**
This is your "wow moment" that wins the hackathon!

**Time investment:**
Spend 3 quality hours on this feature. Make it polished and prominent.

---

## ⏰ Realistic Timeline (Revised)

### Original Plan vs Reality

| Component | Original | Refined | Savings |
|-----------|----------|---------|---------|
| Rust domain models | 2h | 0h | Use JSON directly |
| Facilitator SDK | 7h | 4h | Simplify wrapper |
| Solana integration | 6h | 3h | Use libraries |
| Demo AI API | 3h | 2h | Single endpoint |
| Dashboard | 8h | 5h | Reuse existing components |
| Documentation | 2h | 1h | README + video only |
| **TOTAL** | **28h** | **15h** | **46% faster** |

### Critical Path Timeline

```
PRE-HACKATHON (3 hours) - DO THIS FIRST!
├── Deploy AllSource to Fly.io (1h)
├── Set up Solana wallet (30m)
├── Scaffold packages (30m)
└── Test everything (1h)

DAY 1 MORNING (6 hours)
├── Core SDK types (1h)
├── AllSource event logger (1h)
├── Solana verifier (2h)
└── x402 middleware (2h)
    → CHECKPOINT: Can log + verify?

DAY 1 AFTERNOON (6 hours)
├── Demo AI API (2h)
├── Simple frontend (1h)
├── Deploy to Vercel (1h)
├── End-to-end testing (1h)
└── Documentation (1h)
    → CHECKPOINT: Deployed and working?

DAY 2 MORNING (6 hours)
├── Dashboard layout (1h)
├── Real-time payment feed (2h)
├── Basic metrics (1.5h)
└── Success rate charts (1.5h)
    → CHECKPOINT: Real-time data showing?

DAY 2 AFTERNOON (6 hours)
├── Time-travel component (3h) ⭐ PRIORITY
├── Deploy dashboard (1h)
├── Demo video (1h)
└── Final submission (1h)
    → CHECKPOINT: Submitted?

TOTAL: 27 hours (fits in 48h hackathon with buffer)
```

---

## 🚀 What To Do RIGHT NOW

### Step 1: Verify Environment (15 minutes)
```bash
# Run setup verification script
cd /Users/decebaldobrica/Projects/allsource/allsource-monorepo
./scripts/x402-setup.sh

# This checks:
# - Bun, Solana CLI, Fly CLI, Vercel CLI
# - Environment variables
# - AllSource connection
# - Solana devnet
# - Build system
```

### Step 2: Review Documentation (30 minutes)
```bash
# Read the main checklist
open docs/x402/HACKATHON_CHECKLIST.md

# Print the quick reference
open docs/x402/QUICK_REFERENCE.md

# Have progress tracker ready
open docs/x402/PROGRESS_TRACKER.md
```

### Step 3: Pre-Hackathon Prep (3 hours)

#### A. Deploy AllSource (1 hour)
```bash
cd services/core

# Deploy to Fly.io
fly launch --name allsource-x402-demo
fly secrets set JWT_SECRET=$(openssl rand -hex 32)
fly deploy

# Save the URL
# ALLSOURCE_URL=https://allsource-x402-demo.fly.dev

# Test it works
curl https://allsource-x402-demo.fly.dev/health
curl -X POST https://allsource-x402-demo.fly.dev/api/v1/events \
  -H "Content-Type: application/json" \
  -d '{"event_type":"test","entity_id":"test-1","data":{}}'
```

**Troubleshooting:**
- If Fly.io fails: Try Railway (`railway init`)
- Check logs: `fly logs`
- Verify CORS enabled (should be default)

#### B. Set Up Solana (30 minutes)
```bash
# Create wallet
solana-keygen new --outfile ~/.config/solana/x402-demo.json
solana config set --keypair ~/.config/solana/x402-demo.json
solana config set --url devnet

# Get wallet address
SOLANA_WALLET=$(solana-keygen pubkey ~/.config/solana/x402-demo.json)
echo "Wallet: $SOLANA_WALLET"

# Fund wallet
solana airdrop 2
solana balance

# Test transaction
solana transfer <test-address> 0.01

# Verify on explorer
# https://explorer.solana.com/?cluster=devnet
```

**Troubleshooting:**
- If airdrop fails: Use https://solfaucet.com/
- Check devnet status: https://status.solana.com/

#### C. Create Environment File (15 minutes)
```bash
# Create .env.local from example
cp .env.example .env.local

# Edit with your values
# ALLSOURCE_URL=https://allsource-x402-demo.fly.dev
# SOLANA_WALLET=<your-wallet-pubkey>
# SOLANA_RPC=https://api.devnet.solana.com
# OPENAI_API_KEY=<your-openai-key>
```

#### D. Scaffold Packages (1 hour)
```bash
# Create SDK package
mkdir -p packages/x402-solana-sdk/src
cd packages/x402-solana-sdk

bun init -y
bun add @solana/web3.js @solana/spl-token zod
bun add -D typescript @types/node

# Create package.json
cat > package.json << 'EOF'
{
  "name": "@allsource/x402-solana-sdk",
  "version": "0.1.0",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": {
    "build": "tsc",
    "dev": "tsc --watch"
  }
}
EOF

# Create tsconfig.json
cat > tsconfig.json << 'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "declaration": true,
    "outDir": "./dist",
    "strict": true,
    "esModuleInterop": true
  },
  "include": ["src/**/*"]
}
EOF

# Create source files
touch src/index.ts src/types.ts src/allsource.ts src/solana.ts src/middleware.ts

# Create demo app
cd ../../apps
bun create next-app x402-demo --typescript --tailwind --app --no-src-dir
cd x402-demo
bun add openai
bun add "@allsource/x402-solana-sdk@workspace:*"

mkdir -p app/api/ai
touch app/api/ai/route.ts

# Test build
cd ../../
bun run build
```

#### E. Final Verification (15 minutes)
```bash
# Run setup script again
./scripts/x402-setup.sh

# Should show all green ✅
```

---

## 🎯 Quality Gates (Don't Skip These!)

### Hour 6 Checkpoint ⚠️
**Stop and verify:**
- [ ] Can log events to AllSource?
  ```bash
  # Test event logging
  curl -X POST https://allsource-x402-demo.fly.dev/api/v1/events \
    -H "Content-Type: application/json" \
    -d '{"event_type":"x402.payment.test","entity_id":"test-1","data":{}}'
  ```
- [ ] Can verify Solana transactions?
  ```bash
  # Test with real transaction signature
  node test-solana-verify.js
  ```
- [ ] SDK builds without errors?
  ```bash
  cd packages/x402-solana-sdk
  bun run build
  # Should complete without errors
  ```

**If ANY are ❌: Stop and fix before continuing!**

### Hour 12 Checkpoint ⚠️
**Stop and verify:**
- [ ] Demo API returns 402?
  ```bash
  curl -X POST https://x402-demo.vercel.app/api/ai \
    -H "Content-Type: application/json" \
    -d '{"prompt":"test"}'
  # Should get 402 status
  ```
- [ ] Can process paid requests?
  ```bash
  curl -X POST https://x402-demo.vercel.app/api/ai \
    -H "Content-Type: application/json" \
    -H "X-PAYMENT: <base64-payment>" \
    -d '{"prompt":"test"}'
  # Should get 200 with AI response
  ```
- [ ] Events logged to AllSource?
  ```bash
  curl https://allsource-x402-demo.fly.dev/api/v1/events/query?event_type=x402.payment.*
  # Should see payment events
  ```

**If ANY are ❌: Stop and fix before continuing!**

### Hour 18 Checkpoint ⚠️
**Stop and verify:**
- [ ] Dashboard shows real-time data?
  - Open dashboard URL
  - Make payment
  - Should appear within 2 seconds
- [ ] WebSocket connected?
  - Check browser console for "WebSocket connected"
  - Green status indicator showing
- [ ] Metrics display correctly?
  - Total payments count accurate
  - Success rate calculation correct

**If ANY are ❌: Stop and fix before continuing!**

### Hour 24 Checkpoint ⚠️
**Stop and verify:**
- [ ] Time-travel works?
  - Select timestamp before payment
  - Shows no payment state
  - Select timestamp after payment
  - Shows complete payment details
- [ ] Demo video recorded?
  - 2 minutes or less
  - Good audio and video quality
  - Shows time-travel feature
- [ ] Submitted to hackathon?
  - All forms filled
  - Confirmation received

---

## 🚨 Emergency Procedures

### If Running Out of Time

**Priority Order:**
1. **RECORD DEMO VIDEO FIRST** (even if incomplete)
2. Focus on time-travel feature (your differentiator)
3. Deploy what you have
4. Write basic README
5. Submit

**Emergency Cuts:**
- ❌ Skip advanced analytics
- ❌ Skip dashboard polish
- ❌ Skip comprehensive docs
- ❌ Skip fraud detection

### If Solana Devnet is Down

**Enable mock mode:**
```typescript
// In src/solana.ts
const MOCK_MODE = process.env.MOCK_SOLANA === 'true';

export async function verifyTransaction(...) {
  if (MOCK_MODE) {
    console.log('Mock mode: returning success');
    return { valid: true, amount: minAmount };
  }
  // ... real verification
}
```

Set in `.env.local`:
```bash
MOCK_SOLANA=true
```

### If WebSocket Doesn't Work

**Polling fallback:**
```typescript
// Instead of WebSocket
useEffect(() => {
  const interval = setInterval(async () => {
    const response = await fetch('/api/payments');
    const data = await response.json();
    setPayments(data);
  }, 3000); // Poll every 3 seconds

  return () => clearInterval(interval);
}, []);
```

### If AllSource Deployment Fails

**Backup plans:**
1. Try Railway: `railway init`
2. Try Render: `render deploy`
3. Use local instance + ngrok
4. Record demo with localhost

### If Blocked > 30 Minutes

**DO THIS:**
1. Log the blocker in PROGRESS_TRACKER.md
2. Ask for help (Discord, community, etc.)
3. Move to next task on checklist
4. Come back to blocker later

**DON'T:**
- ❌ Keep trying the same thing
- ❌ Skip documenting the issue
- ❌ Waste the entire hackathon on one bug

---

## 🎬 Demo Video Script (The Winning Formula)

### Recording Setup Checklist
- [ ] 1080p recording quality (Loom or OBS)
- [ ] Test microphone audio levels
- [ ] Close all unnecessary apps/tabs
- [ ] Clear browser history (clean URLs)
- [ ] Have script visible on second monitor
- [ ] Test run through once before recording

### Shot-by-Shot Breakdown (2:00 total)

**Shot 1: The Hook (0:00-0:15)**
```
[Screen: VS Code with README]

"Building x402 payment infrastructure is complex.
You need verification, state management, fraud detection,
compliance logging... that's months of work.

[Zoom into code]

Unless you use AllSource x402."
```

**Shot 2: Code Simplicity (0:15-0:30)**
```
[Screen: Terminal]
$ bun add @allsource/x402-solana-sdk

[Screen: VS Code showing middleware code]
app.use(x402({
  prices: { '/api/ai': 0.01 }
}))

"Three lines of code. That's it. Your API now requires payment."
```

**Shot 3: Demo Payment Flow (0:30-0:50)**
```
[Screen: Postman or curl]

POST /api/ai
→ 402 Payment Required

[Add X-PAYMENT header with real Solana signature]

POST /api/ai
→ 200 OK + AI response

"Payment verified on Solana in under 2 seconds."
```

**Shot 4: Real-Time Dashboard (0:50-1:10)**
```
[Screen: Dashboard]

"Everything's logged in real-time to AllSource event store..."

[Payment appears in feed]

"There's our payment. Status: verified."

[Click Solana Explorer link]

"With full blockchain proof."
```

**Shot 5: TIME-TRAVEL ⭐ (1:10-1:40)**
```
[Screen: Dashboard, click Time Travel button]

"But here's where it gets REALLY interesting.

AllSource is an event store, which means we can
reconstruct state at ANY point in time.

[Select timestamp from 5 minutes ago]
[Click Time Travel button]

Let's go back to when this payment was first requested..."

[State reconstruction appears]

"There it is. The EXACT state at that moment.

What were the requirements? What was the balance?
What verification checks ran?

[Highlight key fields in the JSON]

This is IMPOSSIBLE with PostgreSQL, MongoDB, or Redis.

Only event sourcing makes this possible.

For disputes, audits, compliance - this is a game-changer."
```

**Shot 6: Wrap-Up (1:40-2:00)**
```
[Screen: Split screen showing code + dashboard]

"AllSource x402 SDK:

✓ Production-ready payment infrastructure
✓ In minutes, not months
✓ Event sourcing with time-travel queries
✓ Real-time analytics
✓ First-class Solana support

[Screen: GitHub repo]

Open source SDK. Managed hosting.
Built for the x402 ecosystem.

Check it out at github.com/yourorg/allsource-x402"
```

**Post-Production:**
- Add captions/subtitles
- Add project name overlay
- Add GitHub link overlay
- Upload to YouTube (unlisted)
- Embed in README

---

## 🎯 Scope Priority Matrix

### MUST HAVE (Do These First)
Priority 1 - Core functionality:
1. ✅ Payment flow (402 → verify → process)
2. ✅ Event logging to AllSource
3. ✅ Time-travel feature
4. ✅ Demo video
5. ✅ Working deployment

### SHOULD HAVE (If Time Permits)
Priority 2 - Competitive advantage:
6. ✅ Real-time dashboard
7. ✅ Basic analytics
8. ✅ Clean UI design
9. ✅ Good documentation
10. ✅ Mobile responsive

### NICE TO HAVE (Skip If Needed)
Priority 3 - Bonus points:
11. ⚠️ Fraud detection
12. ⚠️ Advanced charts
13. ⚠️ Multiple payment methods
14. ⚠️ Comprehensive tests
15. ⚠️ Production hardening

**If running out of time, cut from bottom up!**

---

## 🏆 Judging Strategy

### How to Win Each Criteria

**Innovation (35% of score)**
- ✅ Emphasize: "First Solana x402 infrastructure"
- ✅ Highlight: Time-travel debugging (unique capability)
- ✅ Show: Event sourcing applied to payments (novel)
- 🎬 Demo: The time-travel feature extensively

**Technical Execution (25% of score)**
- ✅ Show: Clean architecture (3-line integration)
- ✅ Prove: Actually works (live demo)
- ✅ Mention: Performance (469K events/sec)
- 🎬 Demo: End-to-end payment flow

**Impact (25% of score)**
- ✅ Explain: Lowers barrier to x402 adoption
- ✅ Show: Enables Solana x402 ecosystem
- ✅ Demonstrate: AllSource advantages (time-travel)
- 🎬 Demo: Real-world use case (pay-per-AI-call)

**Presentation (15% of score)**
- ✅ Clear problem statement
- ✅ Compelling demo video
- ✅ WOW moment (time-travel)
- ✅ Professional polish
- 🎬 Demo: Smooth, rehearsed presentation

### Key Messages to Emphasize

**For developers:**
"3 lines of code to add x402 payments"

**For businesses:**
"Production-ready payment infrastructure"

**For judges:**
"Time-travel debugging - impossible with traditional databases"

**For AllSource:**
"First real-world application showcasing event sourcing advantages"

---

## 💪 Motivational Reminders

### You Have Major Advantages

**70% Already Done:**
- ✅ AllSource core is built and tested
- ✅ Event store performs at 469K events/sec
- ✅ Time-travel queries already work
- ✅ Real-time streaming already works
- ✅ UI components already exist

**You're Just Building:**
- 🔨 A wrapper SDK (x402 → AllSource)
- 🔨 Payment verification logic
- 🔨 A demo application
- 🔨 A dashboard UI
- 🔨 Documentation

### Your Competitive Edge

**What makes this winner material:**
1. **Novelty**: First Solana x402 infrastructure
2. **Technical merit**: Time-travel debugging (impossible elsewhere)
3. **Practical value**: 3 lines of code to add payments
4. **Completeness**: Full working demo + dashboard
5. **Polish**: Professional video + docs

### Mindset for Success

**Do:**
- ✅ Focus on the wow factor (time-travel)
- ✅ Ship working code over perfect code
- ✅ Ask for help when blocked > 30 min
- ✅ Take breaks to stay fresh
- ✅ Update progress tracker regularly

**Don't:**
- ❌ Try to build everything
- ❌ Get stuck perfecting one thing
- ❌ Skip the checkpoints
- ❌ Forget to record the demo video
- ❌ Submit at the last second

---

## 📞 Quick Reference (Keep Visible)

### Essential Commands
```bash
# Start development
cd packages/x402-solana-sdk && bun run dev    # Terminal 1
cd apps/x402-demo && bun run dev              # Terminal 2
cd apps/x402-dashboard && bun run dev         # Terminal 3

# Test endpoints
curl https://allsource-x402-demo.fly.dev/health
curl -X POST http://localhost:3000/api/ai -H "Content-Type: application/json" -d '{"prompt":"test"}'
solana balance --url devnet

# Deploy
cd apps/x402-demo && vercel --prod
cd apps/x402-dashboard && vercel --prod

# Verify setup
./scripts/x402-setup.sh
```

### Essential URLs
- **AllSource API:** `https://allsource-x402-demo.fly.dev`
- **Solana Explorer:** `https://explorer.solana.com/?cluster=devnet`
- **Vercel Dashboard:** `https://vercel.com/dashboard`
- **Fly.io Dashboard:** `https://fly.io/dashboard`

### Essential Files
- **Main Checklist:** `docs/x402/HACKATHON_CHECKLIST.md`
- **Quick Reference:** `docs/x402/QUICK_REFERENCE.md`
- **Progress Tracker:** `docs/x402/PROGRESS_TRACKER.md`
- **This Battle Plan:** `docs/x402/BATTLE_PLAN.md`

---

## 🎯 Final Pre-Flight Checklist

Before starting the hackathon:

**Environment:**
- [ ] AllSource deployed and responding
- [ ] Solana wallet created and funded
- [ ] .env.local configured with all variables
- [ ] Setup script passes all checks
- [ ] Build system works

**Documentation:**
- [ ] Read HACKATHON_CHECKLIST.md
- [ ] Printed QUICK_REFERENCE.md
- [ ] Have PROGRESS_TRACKER.md open
- [ ] Understand the timeline
- [ ] Know the emergency procedures

**Mental Preparation:**
- [ ] Clear schedule for 24-48 hours
- [ ] Comfortable workspace set up
- [ ] Snacks and drinks ready
- [ ] Support system informed
- [ ] Excited and ready to build! 🚀

---

## 🚀 You're Ready to Win!

**Remember:**
- 🕰️ Time-travel is your killer feature - spend time on it
- 📹 Demo video quality matters more than code perfection
- 🚀 Shipping beats perfecting
- 💪 You have 70% done already with AllSource
- 🎯 Follow the checkpoints - they keep you on track
- 🛟 Ask for help if blocked > 30 minutes
- 🌟 Focus on the wow factor

**You've got this! Go build something amazing! 🚀**

---

**Last Updated:** [Date you start]
**Status:** Pre-Hackathon Prep
**Next Action:** Run ./scripts/x402-setup.sh

---

## 📋 Quick Action Checklist

**Right now (next 30 minutes):**
- [ ] Run `./scripts/x402-setup.sh`
- [ ] Review `HACKATHON_CHECKLIST.md`
- [ ] Print `QUICK_REFERENCE.md`

**This week (pre-hackathon prep):**
- [ ] Deploy AllSource to Fly.io
- [ ] Set up Solana wallet
- [ ] Scaffold packages
- [ ] Test everything

**Hackathon Day 1:**
- [ ] Build SDK (Hour 0-6)
- [ ] Build demo API (Hour 6-12)

**Hackathon Day 2:**
- [ ] Build dashboard (Hour 12-18)
- [ ] Build time-travel + submit (Hour 18-24)

**You've got this! 🏆**
