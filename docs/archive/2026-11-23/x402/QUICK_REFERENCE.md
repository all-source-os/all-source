---
title: "AllSource x402 - Quick Reference Card"
status: CURRENT
last_updated: 2026-02-02
category: project
project: x402-hackathon
---

# 🚀 AllSource x402 - Quick Reference Card

**Print this and keep it visible during the hackathon!**

---

## 🎯 Project Goal
Build x402 payment SDK for Solana that showcases AllSource event store's time-travel capabilities.

---

## ⏰ Critical Timestamps

| Milestone | Target Time | Status |
|-----------|-------------|--------|
| Pre-hackathon prep done | Before Day 0 | [ ] |
| SDK core complete | Hour 6 | [ ] |
| Demo API deployed | Hour 12 | [ ] |
| Dashboard live | Hour 18 | [ ] |
| Demo video recorded | Hour 23 | [ ] |
| Submitted | Hour 24 | [ ] |

---

## 🔑 Essential Commands

### Start Development
```bash
# Terminal 1: Run demo app
cd apps/x402-demo && bun run dev

# Terminal 2: Build SDK
cd packages/x402-solana-sdk && bun run dev

# Terminal 3: Run dashboard
cd apps/x402-dashboard && bun run dev
```

### Test Endpoints
```bash
# Test AllSource health
curl https://allsource-x402-demo.fly.dev/health

# Test 402 response
curl -X POST http://localhost:3000/api/ai \
  -H "Content-Type: application/json" \
  -d '{"prompt": "test"}'

# Check Solana balance
solana balance
```

### Deploy
```bash
# Deploy demo
cd apps/x402-demo && vercel --prod

# Deploy dashboard
cd apps/x402-dashboard && vercel --prod
```

---

## 🌐 Important URLs

**AllSource API:** `https://allsource-x402-demo.fly.dev`

**Solana Explorer:** `https://explorer.solana.com/?cluster=devnet`

**Demo API:** `_____________________________________`

**Dashboard:** `_____________________________________`

**GitHub Repo:** `_____________________________________`

---

## 🔐 Environment Variables

```bash
# .env.local (all apps)
ALLSOURCE_URL=https://allsource-x402-demo.fly.dev
SOLANA_WALLET=<your-wallet-pubkey>
SOLANA_RPC=https://api.devnet.solana.com
OPENAI_API_KEY=<your-key>

# Dashboard only
NEXT_PUBLIC_ALLSOURCE_URL=https://allsource-x402-demo.fly.dev
NEXT_PUBLIC_ALLSOURCE_WS=wss://allsource-x402-demo.fly.dev/api/v1/events/stream
```

---

## 🚦 Quality Gates

### Hour 6 Checkpoint
- [ ] Can log events to AllSource?
- [ ] Can verify Solana transactions?
- [ ] SDK builds without errors?

### Hour 12 Checkpoint
- [ ] Demo API returns 402?
- [ ] Can process paid requests?
- [ ] Events logged to AllSource?

### Hour 18 Checkpoint
- [ ] Dashboard shows real-time data?
- [ ] WebSocket connected?
- [ ] Metrics display correctly?

### Hour 24 Checkpoint
- [ ] Time-travel works?
- [ ] Demo video recorded?
- [ ] Submitted to hackathon?

---

## 💡 The WOW Factor (Time-Travel)

**What it does:** Reconstruct exact payment state at ANY point in time

**Why it matters:** Impossible with PostgreSQL/MongoDB/Redis

**Demo script:**
1. Show payment in feed
2. Click "Time Travel" button
3. Select timestamp BEFORE payment
4. Show: no payment exists
5. Select timestamp AFTER payment
6. Show: complete payment details
7. Emphasize: "This is only possible with event sourcing"

---

## 🚨 Emergency Contacts

**Solana devnet down?** → Enable mock mode: `MOCK_SOLANA=true`

**WebSocket fails?** → Use polling fallback (see checklist)

**Out of time?** → Record video FIRST, then polish

**Stuck > 30 min?** → Ask for help in community!

---

## 📋 Pre-Recording Checklist

- [ ] Close unnecessary apps
- [ ] Clear browser history
- [ ] Test audio levels
- [ ] 1080p recording quality
- [ ] Script in front of you
- [ ] Demo URLs work
- [ ] Time-travel feature ready

---

## 🎬 Video Script (2 minutes)

**0:00-0:30** → Problem + 3-line solution
**0:30-0:50** → Make paid AI call
**0:50-1:10** → Dashboard real-time update
**1:10-1:40** → ⭐ TIME-TRAVEL DEMO ⭐
**1:40-2:00** → Wrap-up + CTA

---

## 🎯 Scope Priority

### MUST HAVE
1. Payment flow (402 → verify → process)
2. Time-travel feature
3. Demo video
4. Working deployment

### SHOULD HAVE
5. Real-time dashboard
6. Basic analytics
7. Good README

### NICE TO HAVE
8. Fraud detection
9. Advanced charts
10. Comprehensive tests

**If running out of time, cut from bottom up!**

---

## 💪 Motivational Reminders

- ✅ AllSource is already built → You're showcasing, not building from scratch
- ✅ Time-travel is your killer feature → Focus here for max impact
- ✅ Good demo video > Perfect code → Ship, don't perfect
- ✅ You have 70% done already → Just need the x402 wrapper

**You've got this! 🚀**

---

## 📞 Help Resources

**AllSource Issues:** Check existing API endpoints in `services/core/src/api.rs`

**Solana Issues:** https://solana.com/developers

**Next.js Issues:** https://nextjs.org/docs

**Deployment Issues:** Vercel dashboard logs

---

**Last Updated:** [Current Date]
**Current Status:** _____________
**Next Action:** _____________
