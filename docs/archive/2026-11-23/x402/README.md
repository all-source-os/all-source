---
title: "AllSource x402 Hackathon Project"
status: CURRENT
last_updated: 2026-02-02
category: project
project: x402-hackathon
---

# 🏆 AllSource x402 Hackathon Project

> Event-sourced payment infrastructure for x402 on Solana - showcasing AllSource event store

---

## 📚 Documentation Index

This directory contains all documentation and planning materials for the AllSource x402 hackathon project.

### 🚀 Getting Started

1. **[HACKATHON_CHECKLIST.md](./HACKATHON_CHECKLIST.md)** - **START HERE!**
   - Complete step-by-step checklist for the entire hackathon
   - Broken down by time blocks (pre-hackathon, Day 1, Day 2)
   - Includes testing checkpoints and troubleshooting

2. **[QUICK_REFERENCE.md](./QUICK_REFERENCE.md)** - **PRINT THIS!**
   - One-page reference card for essential commands and URLs
   - Emergency procedures
   - Quality gates and checkpoints

3. **[PROGRESS_TRACKER.md](./PROGRESS_TRACKER.md)** - **UPDATE REGULARLY!**
   - Live progress tracking document
   - Time budget monitoring
   - Issues log

4. **[BATTLE_PLAN.md](./BATTLE_PLAN.md)** - **YOUR MASTER REFERENCE**
   - Complete strategy overview
   - Pre-hackathon prep steps
   - Quality gates and checkpoints
   - Emergency procedures

### 💼 SaaS Product Planning

5. **[PRD.md](./PRD.md)** - **Product Requirements Document**
   - Formal PRD for AllSource Paywall SaaS
   - User stories, features, technical requirements
   - Timeline, metrics, and success criteria
   - Use this for building the actual product

6. **[SAAS_STRATEGY.md](./SAAS_STRATEGY.md)** - **Strategic Planning**
   - Market analysis and competitive positioning
   - Go-to-market strategy
   - Business model and pricing
   - Platform integration strategies
   - Event sourcing use cases and advantages

### 🔧 Setup

**Pre-Hackathon Setup Script:**
```bash
# From project root
./scripts/x402-setup.sh
```

This script checks:
- ✅ Prerequisites (Bun, Solana CLI, Fly CLI, Vercel CLI)
- ✅ Environment variables
- ✅ AllSource deployment
- ✅ Solana devnet connection
- ✅ Project structure
- ✅ Build system

**Environment Configuration:**
```bash
# Copy example and fill in your values
cp .env.example .env.local

# Required variables:
# - ALLSOURCE_URL
# - SOLANA_WALLET
# - OPENAI_API_KEY
```

---

## 🎯 Project Overview

### What We're Building

**AllSource x402 SDK** - Complete infrastructure toolkit for building x402 payment systems on Solana, powered by event sourcing.

### Components

1. **TypeScript SDK** (`packages/x402-solana-sdk/`)
   - Drop-in facilitator middleware
   - Solana payment verification
   - Automatic event logging to AllSource

2. **Demo AI API** (`apps/x402-demo/`)
   - Pay-per-call AI API
   - Shows real-world x402 payment flow
   - OpenAI integration

3. **Analytics Dashboard** (`apps/x402-dashboard/`)
   - Real-time payment monitoring
   - Time-travel debugging UI
   - Payment analytics

### The Killer Feature: Time-Travel Debugging 🕰️

**What it does:** Reconstruct the exact state of any payment at any point in time.

**Why it matters:** This is impossible with traditional databases (PostgreSQL, MongoDB, Redis). Only event sourcing enables this.

**Use cases:**
- Dispute resolution
- Compliance audits
- Debugging payment failures
- Historical analysis

**Demo impact:** This is your competitive differentiator!

---

## ⏰ Timeline

### Pre-Hackathon (3 hours) - **DO NOT SKIP!**
- Deploy AllSource
- Set up Solana wallet
- Scaffold project structure
- Verify everything works

### Day 1 (12 hours)
- **Morning:** Core SDK + middleware (Hour 0-6)
- **Afternoon:** Demo API + deployment (Hour 6-12)

### Day 2 (12 hours)
- **Morning:** Dashboard + analytics (Hour 12-18)
- **Afternoon:** Time-travel feature + submission (Hour 18-24)

---

## 🎯 Success Criteria

### Must Have (Required to Win)
1. ✅ Working payment flow (402 → verify → process)
2. ✅ Time-travel feature functional
3. ✅ Demo video (2 min max)
4. ✅ Deployed and accessible

### Should Have (Competitive Advantage)
5. ✅ Real-time dashboard
6. ✅ Basic analytics
7. ✅ Clean UI
8. ✅ Good documentation

### Nice to Have (Bonus Points)
9. ⚠️ Fraud detection
10. ⚠️ Advanced charts
11. ⚠️ Comprehensive tests

---

## 🚀 Quick Start

### Development Commands

```bash
# Start SDK development
cd packages/x402-solana-sdk
bun run dev

# Run demo app
cd apps/x402-demo
bun run dev

# Run dashboard
cd apps/x402-dashboard
bun run dev

# Build all packages
bun run build
```

### Testing Commands

```bash
# Test AllSource health
curl https://allsource-x402-demo.fly.dev/health

# Test 402 response
curl -X POST http://localhost:3000/api/ai \
  -H "Content-Type: application/json" \
  -d '{"prompt": "test"}'

# Check Solana balance
solana balance --url devnet

# Run setup verification
./scripts/x402-setup.sh
```

### Deployment Commands

```bash
# Deploy demo API
cd apps/x402-demo
vercel --prod

# Deploy dashboard
cd apps/x402-dashboard
vercel --prod

# Deploy AllSource (one-time)
cd services/core
fly deploy
```

---

## 📊 Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    CLIENT REQUEST                         │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│              Next.js API Route (Demo)                    │
│  • Receives request                                      │
│  • Applies x402 middleware                               │
│  • Returns 402 or processes with OpenAI                  │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│          @allsource/x402-solana-sdk                        │
│  ┌─────────────────────────────────────────────┐        │
│  │ x402 Middleware                              │        │
│  │  • Check for payment header                  │        │
│  │  • Return 402 if missing                     │        │
│  │  • Verify payment if present                 │        │
│  └──────────┬──────────────────────────────────┘        │
│             │                                             │
│    ┌────────┴────────┐                                   │
│    ▼                 ▼                                    │
│  ┌─────────┐    ┌─────────┐                             │
│  │ Solana  │    │ AllSource │                             │
│  │ Verify  │    │ Logger  │                             │
│  └─────────┘    └─────────┘                             │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│           AllSource Event Store (Hosted)                   │
│  ┌─────────────────────────────────────────────┐        │
│  │ Payment Events (Parquet Storage)             │        │
│  │  • x402.payment.requested                    │        │
│  │  • x402.payment.submitted                    │        │
│  │  • x402.payment.verified                     │        │
│  │  • x402.payment.failed                       │        │
│  └─────────────────────────────────────────────┘        │
│                                                           │
│  Features:                                                │
│  • 469K events/sec ingestion                             │
│  • Time-travel queries (the wow factor!)                 │
│  • Real-time WebSocket streaming                         │
│  • Multi-tenant isolation                                │
└──────────────────────┬──────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────┐
│            Analytics Dashboard (Next.js)                 │
│  • Live payment feed (WebSocket)                         │
│  • Success rate metrics                                  │
│  • Time-travel debugger 🕰️                              │
│  • Fraud alerts                                          │
└─────────────────────────────────────────────────────────┘
```

---

## 🎬 Demo Video Script (2 minutes)

### Shot 1: Hook (0:00-0:15)
Show the problem + solution code

### Shot 2: Demo Payment (0:15-0:50)
Make a paid AI API call

### Shot 3: Real-Time Dashboard (0:50-1:10)
Show payment appearing in dashboard

### Shot 4: Time-Travel ⭐ (1:10-1:40)
The wow moment - reconstruct historical state

### Shot 5: Wrap-Up (1:40-2:00)
Value proposition + call to action

**Recording Tips:**
- 1080p quality
- Test audio first
- Close unnecessary apps
- Practice once before recording

---

## 🚨 Emergency Procedures

### If Solana Devnet is Down
Enable mock mode: `MOCK_SOLANA=true`

### If WebSocket Fails
Use polling fallback (see checklist)

### If Running Out of Time
1. Record demo video FIRST
2. Focus on time-travel feature
3. Deploy what you have
4. Submit incomplete but functional

### If Blocked > 30 Minutes
- Ask for help in community
- Document the blocker
- Move to next task
- Come back later

---

## 📞 Resources

### Hackathon Documents
- [Complete Checklist](./HACKATHON_CHECKLIST.md)
- [Quick Reference](./QUICK_REFERENCE.md)
- [Progress Tracker](./PROGRESS_TRACKER.md)

### Technical Documentation
- [x402 Specification](https://github.com/coinbase/x402)
- [Solana Developers](https://solana.com/developers)
- [Next.js Docs](https://nextjs.org/docs)
- [AllSource Core API](../../services/core/src/api.rs)

### External Links
- [x402 Foundation](https://x402.org)
- [Solana Explorer](https://explorer.solana.com/?cluster=devnet)
- [Vercel Dashboard](https://vercel.com/dashboard)
- [Fly.io Dashboard](https://fly.io/dashboard)

---

## 💡 Tips for Success

### Before Starting
- ✅ Complete pre-hackathon setup
- ✅ Print quick reference card
- ✅ Test all connections
- ✅ Get good sleep

### During Hackathon
- ⏰ Update progress tracker every 2-3 hours
- 🚨 Log blockers immediately
- 🎯 Focus on must-haves first
- 📹 Record demo video early (Day 1 if possible)
- 💪 Take breaks when needed

### Key Focus Areas
1. **Time-travel feature** - This is your differentiator!
2. **Demo video quality** - This sells the project
3. **Working payment flow** - Core functionality first
4. **Real-time dashboard** - Shows event sourcing power

### What to Skip
- ❌ Comprehensive tests
- ❌ Production hardening
- ❌ Multiple payment methods
- ❌ Advanced fraud detection

---

## 🏆 Judging Strategy

### Innovation (35%)
- ✅ First Solana x402 infrastructure
- ✅ Time-travel debugging (unique!)
- ✅ Event-sourced payments

### Technical Execution (25%)
- ✅ Clean architecture
- ✅ Working end-to-end demo
- ✅ Performance (469K events/sec)

### Impact (25%)
- ✅ Lowers barrier to x402 adoption
- ✅ Enables Solana x402 ecosystem
- ✅ Showcases AllSource advantages

### Presentation (15%)
- ✅ Clear value proposition
- ✅ Live working demo
- ✅ **WOW moment** (time-travel)

---

## 📈 Post-Hackathon

### If You Win
- 🎉 Celebrate!
- 📱 Share on social media
- 🤝 Connect with judges/mentors
- 📊 Gather feedback

### If You Don't Win
- 💪 You still built something amazing!
- 📝 Write a blog post about learnings
- 🚀 Continue development
- 🌟 Open-source the code

### Next Steps
- Polish the documentation
- Add comprehensive tests
- Implement fraud detection
- Support multiple blockchains
- Launch as real product

---

## 🤝 Contributing

After the hackathon, we welcome contributions:
- Bug reports and fixes
- Feature requests
- Documentation improvements
- Examples and tutorials

---

## 📄 License

MIT License - See LICENSE file for details

---

## 🙏 Acknowledgments

- **AllSource Team** - For the amazing event store
- **x402 Foundation** - For the payment standard
- **Solana Foundation** - For the blockchain platform
- **Hackathon Organizers** - For the opportunity

---

**Remember:**
- 🕰️ Time-travel is your killer feature
- 📹 Demo video quality matters
- 🚀 Ship, don't perfect
- 💪 You've got 70% done already

**Good luck! Go build something amazing! 🚀**
