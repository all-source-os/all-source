---
title: "AllSource x402 Hackathon - Project Checklist"
status: CURRENT
last_updated: 2026-02-02
category: project
project: x402-hackathon
---

# 🏆 AllSource x402 Hackathon - Project Checklist

**Project:** AllSource x402 SDK - Solana Payment Infrastructure
**Timeline:** 24-48 hours
**Goal:** Showcase AllSource event store for x402 payment systems

---

## 📅 PRE-HACKATHON PREPARATION (CRITICAL!)

> ⚠️ **DO NOT SKIP!** This saves 4+ hours during the hackathon.

### Week Before Hackathon

#### 🚀 Infrastructure Setup (2-3 hours)

**AllSource Deployment**
- [ ] Deploy AllSource core to Fly.io/Railway
  ```bash
  cd services/core
  fly launch --name allsource-x402-demo
  fly secrets set JWT_SECRET=$(openssl rand -hex 32)
  fly deploy
  ```
- [ ] Save deployment URL: `_______________________________________________`
- [ ] Test health endpoint: `curl https://allsource-x402-demo.fly.dev/health`
- [ ] Verify response: `{"status":"healthy","service":"allsource-core"}`

**Event Ingestion Test**
- [ ] Test POST to `/api/v1/events`
  ```bash
  curl -X POST https://allsource-x402-demo.fly.dev/api/v1/events \
    -H "Content-Type: application/json" \
    -d '{
      "event_type": "test.event",
      "entity_id": "test-1",
      "timestamp": '$(date +%s000)',
      "data": {"test": true}
    }'
  ```
- [ ] Verify 200 response received
- [ ] Test GET query: `curl https://allsource-x402-demo.fly.dev/api/v1/events/query`
- [ ] Confirm event appears in results

**WebSocket Test**
- [ ] Install wscat: `bun add -g wscat`
- [ ] Test WebSocket connection:
  ```bash
  wscat -c wss://allsource-x402-demo.fly.dev/api/v1/events/stream
  ```
- [ ] Send test event (from another terminal)
- [ ] Verify event appears in WebSocket stream
- [ ] Confirm no connection errors

**Troubleshooting:**
- [ ] If deployment fails, try Railway: `railway init`
- [ ] Check Fly.io logs: `fly logs`
- [ ] Verify CORS is enabled (it should be by default)

---

#### 💰 Solana Setup (30 minutes)

**Create Wallet**
- [ ] Generate new keypair:
  ```bash
  solana-keygen new --outfile ~/.config/solana/x402-demo.json
  ```
- [ ] Save wallet address: `_______________________________________________`
- [ ] Set as default: `solana config set --keypair ~/.config/solana/x402-demo.json`
- [ ] Switch to devnet: `solana config set --url devnet`

**Fund Wallet**
- [ ] Airdrop SOL:
  ```bash
  solana airdrop 2
  solana balance
  ```
- [ ] Verify balance: `_____ SOL`
- [ ] Request USDC devnet tokens (if available)
- [ ] Save USDC token mint: `_______________________________________________`

**Test Transaction**
- [ ] Create test transaction:
  ```bash
  solana transfer <recipient-address> 0.01
  ```
- [ ] Save transaction signature: `_______________________________________________`
- [ ] Verify on explorer: https://explorer.solana.com/tx/SIGNATURE?cluster=devnet
- [ ] Confirm transaction shows "Success" status

**Troubleshooting:**
- [ ] If airdrop fails, use: https://solfaucet.com/
- [ ] Check devnet status: https://status.solana.com/
- [ ] Try alternative RPC: https://api.devnet.solana.com

---

#### 📦 Project Scaffolding (1 hour)

**Create SDK Package**
- [ ] Create package directory:
  ```bash
  cd packages
  mkdir x402-solana-sdk
  cd x402-solana-sdk
  ```
- [ ] Initialize package:
  ```bash
  bun init -y
  ```
- [ ] Install dependencies:
  ```bash
  bun add @solana/web3.js @solana/spl-token zod
  bun add -D typescript @types/node
  ```
- [ ] Create package.json:
  ```json
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
  ```
- [ ] Create tsconfig.json:
  ```json
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
  ```
- [ ] Create src directory structure:
  ```bash
  mkdir -p src
  touch src/index.ts src/types.ts src/allsource.ts src/solana.ts src/middleware.ts
  ```

**Create Demo App**
- [ ] Create Next.js app:
  ```bash
  cd ../../apps
  bun create next-app x402-demo --typescript --tailwind --app --no-src-dir
  cd x402-demo
  ```
- [ ] Install dependencies:
  ```bash
  bun add openai
  bun add -D @types/node
  ```
- [ ] Add SDK to workspace:
  ```json
  // In package.json dependencies
  "@allsource/x402-solana-sdk": "workspace:*"
  ```
- [ ] Create API route directory:
  ```bash
  mkdir -p app/api/ai
  touch app/api/ai/route.ts
  ```

**Create Dashboard App**
- [ ] Copy web app as template:
  ```bash
  cd ../../apps
  cp -r web x402-dashboard
  cd x402-dashboard
  ```
- [ ] Update package.json name: `"@allsource/x402-dashboard"`
- [ ] Clean up unnecessary components
- [ ] Create dashboard page structure:
  ```bash
  mkdir -p app/dashboard
  touch app/dashboard/page.tsx
  mkdir -p components/dashboard
  touch components/dashboard/payment-feed.tsx
  touch components/dashboard/time-travel.tsx
  ```

**Verify Build System**
- [ ] Build SDK: `cd packages/x402-solana-sdk && bun run build`
- [ ] Build demo app: `cd apps/x402-demo && bun run build`
- [ ] Build dashboard: `cd apps/x402-dashboard && bun run build`
- [ ] Run turbo build from root: `bun run build`
- [ ] Confirm all packages build successfully

**Troubleshooting:**
- [ ] If workspace linking fails, run: `bun install` from root
- [ ] Check turbo.json includes new packages
- [ ] Verify tsconfig paths are correct

---

#### 🧪 Environment Testing (30 minutes)

**Create Test Script**
- [ ] Create `packages/x402-solana-sdk/test-setup.ts`:
  ```typescript
  import { Connection, PublicKey } from '@solana/web3.js';

  async function testSetup() {
    // Test AllSource
    console.log('Testing AllSource...');
    const allsourceUrl = process.env.ALLSOURCE_URL || 'https://allsource-x402-demo.fly.dev';
    const response = await fetch(`${allsourceUrl}/health`);
    console.log('✅ AllSource:', response.status === 200 ? 'OK' : 'FAILED');

    // Test Solana
    console.log('Testing Solana devnet...');
    const connection = new Connection('https://api.devnet.solana.com');
    const slot = await connection.getSlot();
    console.log('✅ Solana devnet:', slot > 0 ? 'OK' : 'FAILED');

    // Test wallet
    const wallet = process.env.SOLANA_WALLET;
    if (wallet) {
      const balance = await connection.getBalance(new PublicKey(wallet));
      console.log('✅ Wallet balance:', balance / 1e9, 'SOL');
    }

    console.log('\n🎉 All systems ready!');
  }

  testSetup().catch(console.error);
  ```
- [ ] Create `.env.local`:
  ```
  ALLSOURCE_URL=https://allsource-x402-demo.fly.dev
  SOLANA_WALLET=<your-wallet-address>
  SOLANA_RPC=https://api.devnet.solana.com
  OPENAI_API_KEY=<your-key>
  ```
- [ ] Run test: `bun run test-setup.ts`
- [ ] Verify all checks pass

**Pre-Hackathon Checklist Summary**
- [ ] ✅ AllSource deployed and responding
- [ ] ✅ Solana wallet funded with devnet SOL
- [ ] ✅ Project structure scaffolded
- [ ] ✅ All dependencies installed
- [ ] ✅ Build system works
- [ ] ✅ Test script passes

---

## 🏃 HACKATHON DAY 1 (12 hours)

### Hour 0-3: Core SDK Foundation

#### Types & Interfaces (30 min)

- [ ] Create `src/types.ts`:
  ```typescript
  export interface X402Payment {
    id: string;
    signature: string;
    amount: number;
    status: 'pending' | 'submitted' | 'verified' | 'failed';
    timestamp: number;
  }

  export interface PaymentRequirement {
    scheme: string;
    network: string;
    maxAmountRequired: number;
    payTo: string;
    resource: string;
  }

  export interface VerificationResult {
    valid: boolean;
    amount?: number;
    error?: string;
  }

  export interface X402Config {
    allsourceUrl: string;
    solanaWallet: string;
    solanaRpc?: string;
    prices: Record<string, number>;
  }
  ```
- [ ] Build and verify no type errors

#### AllSource Event Logger (1 hour)

- [ ] Create `src/allsource.ts`:
  ```typescript
  export class AllSourceEventLogger {
    constructor(private allsourceUrl: string) {}

    async logPaymentEvent(
      type: 'requested' | 'submitted' | 'verified' | 'failed',
      data: any
    ) {
      const response = await fetch(`${this.allsourceUrl}/api/v1/events`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          event_type: `x402.payment.${type}`,
          entity_id: data.payment_id,
          timestamp: Date.now(),
          data
        })
      });

      if (!response.ok) {
        throw new Error(`Failed to log event: ${response.statusText}`);
      }

      return response.json();
    }
  }
  ```
- [ ] Test logging:
  ```typescript
  const logger = new AllSourceEventLogger('https://allsource-x402-demo.fly.dev');
  await logger.logPaymentEvent('requested', {
    payment_id: 'test-123',
    amount: 0.01
  });
  ```
- [ ] Verify event appears in AllSource via query API
- [ ] Check event structure is correct

#### Solana Transaction Verifier (1.5 hours)

- [ ] Create `src/solana.ts`:
  ```typescript
  import { Connection, PublicKey } from '@solana/web3.js';

  export async function verifyTransaction(
    signature: string,
    expectedRecipient: string,
    minAmount: number,
    rpcUrl: string = 'https://api.devnet.solana.com'
  ): Promise<{ valid: boolean; amount?: number; error?: string }> {
    try {
      const connection = new Connection(rpcUrl);

      const tx = await connection.getTransaction(signature, {
        commitment: 'confirmed',
        maxSupportedTransactionVersion: 0
      });

      if (!tx) {
        return { valid: false, error: 'Transaction not found' };
      }

      if (tx.meta?.err) {
        return { valid: false, error: 'Transaction failed' };
      }

      // Basic validation (enhance with SPL token logic later)
      const postBalances = tx.meta.postBalances;
      const preBalances = tx.meta.preBalances;

      const amount = Math.abs(postBalances[1] - preBalances[1]);

      return {
        valid: amount >= minAmount,
        amount
      };
    } catch (error) {
      return {
        valid: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }
  ```
- [ ] Test with real transaction signature
- [ ] Verify returns correct validation result
- [ ] Test error cases (invalid signature, failed tx)

**Checkpoint at Hour 3:**
- [ ] Can log events to AllSource successfully?
- [ ] Can verify Solana transactions?
- [ ] Types compile without errors?
- [ ] Test script shows all green?

**If blocked:** Document issue and move to next section, come back later.

---

### Hour 3-6: Middleware Implementation

#### 402 Middleware Core (2 hours)

- [ ] Create `src/middleware.ts`:
  ```typescript
  import { AllSourceEventLogger } from './allsource';
  import { verifyTransaction } from './solana';
  import type { X402Config, PaymentRequirement } from './types';

  export function createX402Middleware(config: X402Config) {
    const logger = new AllSourceEventLogger(config.allsourceUrl);

    return async function x402Middleware(
      req: any,
      res: any,
      next: () => void
    ) {
      const price = config.prices[req.path];

      if (!price) {
        return next();
      }

      const paymentHeader = req.headers['x-payment'];
      const paymentId = `payment-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

      if (!paymentHeader) {
        await logger.logPaymentEvent('requested', {
          payment_id: paymentId,
          path: req.path,
          price
        });

        const requirement: PaymentRequirement = {
          scheme: 'solana-pay',
          network: 'solana-devnet',
          maxAmountRequired: price * 1_000_000,
          payTo: config.solanaWallet,
          resource: req.path
        };

        return res.status(402).json({
          x402Version: '1.0',
          accepts: [requirement]
        });
      }

      try {
        const payment = JSON.parse(
          Buffer.from(paymentHeader as string, 'base64').toString()
        );

        await logger.logPaymentEvent('submitted', {
          payment_id: paymentId,
          signature: payment.signature,
          amount: price
        });

        const verification = await verifyTransaction(
          payment.signature,
          config.solanaWallet,
          price * 1_000_000,
          config.solanaRpc
        );

        if (verification.valid) {
          await logger.logPaymentEvent('verified', {
            payment_id: paymentId,
            signature: payment.signature,
            amount: verification.amount
          });

          (req as any).payment = { id: paymentId, ...verification };
          next();
        } else {
          await logger.logPaymentEvent('failed', {
            payment_id: paymentId,
            reason: verification.error || 'verification_failed'
          });

          res.status(402).json({ error: 'Payment verification failed' });
        }
      } catch (error) {
        await logger.logPaymentEvent('failed', {
          payment_id: paymentId,
          reason: error instanceof Error ? error.message : 'unknown_error'
        });

        res.status(400).json({ error: 'Invalid payment header' });
      }
    };
  }
  ```
- [ ] Build SDK: `bun run build`
- [ ] Fix any TypeScript errors

#### Export Public API (30 min)

- [ ] Update `src/index.ts`:
  ```typescript
  export { createX402Middleware } from './middleware';
  export { AllSourceEventLogger } from './allsource';
  export { verifyTransaction } from './solana';
  export type {
    X402Config,
    X402Payment,
    PaymentRequirement,
    VerificationResult
  } from './types';
  ```
- [ ] Build and verify exports work
- [ ] Create README.md with basic usage example

**Checkpoint at Hour 6:**
- [ ] SDK builds without errors?
- [ ] Exports are accessible?
- [ ] Can import SDK in demo app?

---

### Hour 6-9: Demo AI API

#### API Route Setup (1 hour)

- [ ] Create `apps/x402-demo/app/api/ai/route.ts`:
  ```typescript
  import { NextRequest, NextResponse } from 'next/server';
  import { createX402Middleware } from '@allsource/x402-solana-sdk';
  import OpenAI from 'openai';

  const openai = new OpenAI({
    apiKey: process.env.OPENAI_API_KEY
  });

  const x402 = createX402Middleware({
    allsourceUrl: process.env.ALLSOURCE_URL!,
    solanaWallet: process.env.SOLANA_WALLET!,
    solanaRpc: process.env.SOLANA_RPC,
    prices: {
      '/api/ai': 0.01
    }
  });

  export async function POST(req: NextRequest) {
    // Apply payment middleware
    let paymentError: NextResponse | null = null;
    let paymentVerified = false;

    const mockRes = {
      status: (code: number) => ({
        json: (data: any) => {
          paymentError = NextResponse.json(data, { status: code });
          return paymentError;
        }
      })
    };

    const mockNext = () => {
      paymentVerified = true;
    };

    await x402(req, mockRes, mockNext);

    if (paymentError) {
      return paymentError;
    }

    if (!paymentVerified) {
      return NextResponse.json(
        { error: 'Payment required' },
        { status: 402 }
      );
    }

    // Payment verified - process AI request
    const { prompt } = await req.json();

    const completion = await openai.chat.completions.create({
      model: 'gpt-3.5-turbo',
      messages: [{ role: 'user', content: prompt }],
      max_tokens: 100
    });

    return NextResponse.json({
      response: completion.choices[0].message.content,
      payment_id: (req as any).payment?.id
    });
  }
  ```
- [ ] Create `.env.local` in demo app:
  ```
  ALLSOURCE_URL=https://allsource-x402-demo.fly.dev
  SOLANA_WALLET=<your-wallet>
  SOLANA_RPC=https://api.devnet.solana.com
  OPENAI_API_KEY=<your-key>
  ```
- [ ] Test locally: `bun run dev`

#### API Testing (1 hour)

- [ ] Test without payment header:
  ```bash
  curl -X POST http://localhost:3000/api/ai \
    -H "Content-Type: application/json" \
    -d '{"prompt": "Hello"}'
  ```
- [ ] Verify 402 response received
- [ ] Check response includes payment requirements
- [ ] Create test payment header with real Solana tx
- [ ] Test with payment header
- [ ] Verify 200 response with AI output
- [ ] Check AllSource for logged events
- [ ] Verify all event types present (requested, submitted, verified)

#### Simple Frontend (1 hour)

- [ ] Create `apps/x402-demo/app/page.tsx`:
  ```typescript
  'use client';

  import { useState } from 'react';

  export default function Home() {
    const [prompt, setPrompt] = useState('');
    const [response, setResponse] = useState('');
    const [error, setError] = useState('');
    const [paymentRequired, setPaymentRequired] = useState(null);

    const handleSubmit = async () => {
      const res = await fetch('/api/ai', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt })
      });

      if (res.status === 402) {
        const data = await res.json();
        setPaymentRequired(data);
        return;
      }

      const data = await res.json();
      setResponse(data.response);
    };

    return (
      <div className="container mx-auto p-8">
        <h1 className="text-4xl font-bold mb-8">
          Pay-per-Call AI API Demo
        </h1>

        <div className="max-w-2xl">
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            className="w-full p-4 border rounded-lg"
            rows={4}
            placeholder="Enter your prompt..."
          />

          <button
            onClick={handleSubmit}
            className="mt-4 px-6 py-3 bg-blue-600 text-white rounded-lg"
          >
            Send (0.01 USDC)
          </button>

          {paymentRequired && (
            <div className="mt-4 p-4 bg-yellow-100 rounded-lg">
              <h3 className="font-bold">Payment Required</h3>
              <pre className="text-sm">
                {JSON.stringify(paymentRequired, null, 2)}
              </pre>
            </div>
          )}

          {response && (
            <div className="mt-4 p-4 bg-green-100 rounded-lg">
              <h3 className="font-bold">Response</h3>
              <p>{response}</p>
            </div>
          )}
        </div>
      </div>
    );
  }
  ```
- [ ] Test UI locally
- [ ] Verify payment requirement message appears

**Checkpoint at Hour 9:**
- [ ] AI API returns 402 for unpaid requests?
- [ ] AI API processes paid requests?
- [ ] Events logged to AllSource?
- [ ] Frontend shows payment requirement?

**If blocked on OpenAI:** Mock the response, focus on payment flow.

---

### Hour 9-12: Deployment & Testing

#### Deploy Demo App (1 hour)

- [ ] Create Vercel account (if needed)
- [ ] Install Vercel CLI: `bun add -g vercel`
- [ ] Deploy:
  ```bash
  cd apps/x402-demo
  vercel --prod
  ```
- [ ] Set environment variables in Vercel dashboard:
  - `ALLSOURCE_URL`
  - `SOLANA_WALLET`
  - `SOLANA_RPC`
  - `OPENAI_API_KEY`
- [ ] Test deployed API:
  ```bash
  curl -X POST https://x402-demo.vercel.app/api/ai \
    -H "Content-Type: application/json" \
    -d '{"prompt": "test"}'
  ```
- [ ] Verify 402 response
- [ ] Save deployment URL: `_______________________________________________`

#### End-to-End Testing (1 hour)

- [ ] Create real Solana payment transaction
- [ ] Encode as base64 payment header
- [ ] Test full payment flow:
  ```bash
  curl -X POST https://x402-demo.vercel.app/api/ai \
    -H "Content-Type: application/json" \
    -H "X-PAYMENT: <base64-encoded-payment>" \
    -d '{"prompt": "Tell me a joke"}'
  ```
- [ ] Verify AI response received
- [ ] Check AllSource for events
- [ ] Verify event sequence (requested → submitted → verified)
- [ ] Test failed payment (invalid signature)
- [ ] Verify failure is logged correctly

#### Documentation (1 hour)

- [ ] Update `packages/x402-solana-sdk/README.md`:
  - Installation instructions
  - Quick start example
  - API reference
  - Usage with Next.js
- [ ] Create `docs/x402/QUICKSTART.md`
- [ ] Add code examples
- [ ] Include curl examples for testing
- [ ] Document environment variables

**Day 1 End Checkpoint:**
- [ ] Core SDK complete and working?
- [ ] Demo API deployed and accessible?
- [ ] Can complete full payment flow?
- [ ] Events appearing in AllSource?
- [ ] Documentation drafted?

**Day 1 Status:** ____ / 100% complete

---

## 🎨 HACKATHON DAY 2 (12 hours)

### Hour 12-15: Dashboard Foundation

#### Dashboard Setup (30 min)

- [ ] Create dashboard layout `apps/x402-dashboard/app/dashboard/page.tsx`
- [ ] Set up Tailwind with dark mode support
- [ ] Create card components for metrics
- [ ] Add navigation header
- [ ] Implement responsive grid layout

#### Real-Time Payment Feed (2.5 hours)

- [ ] Create `components/dashboard/payment-feed.tsx`:
  ```typescript
  'use client';

  import { useEffect, useState } from 'react';
  import { Card } from '@/components/ui/card';

  interface PaymentEvent {
    event_id: string;
    event_type: string;
    entity_id: string;
    timestamp: number;
    data: any;
  }

  export function PaymentFeed() {
    const [payments, setPayments] = useState<PaymentEvent[]>([]);
    const [wsStatus, setWsStatus] = useState<'connecting' | 'connected' | 'disconnected'>('connecting');

    useEffect(() => {
      const wsUrl = process.env.NEXT_PUBLIC_ALLSOURCE_WS ||
        'wss://allsource-x402-demo.fly.dev/api/v1/events/stream';

      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        console.log('WebSocket connected');
        setWsStatus('connected');
      };

      ws.onclose = () => {
        console.log('WebSocket disconnected');
        setWsStatus('disconnected');
      };

      ws.onerror = (error) => {
        console.error('WebSocket error:', error);
        setWsStatus('disconnected');
      };

      ws.onmessage = (event) => {
        const data = JSON.parse(event.data);

        if (data.event_type.startsWith('x402.payment')) {
          setPayments(prev => [data, ...prev].slice(0, 50));
        }
      };

      return () => ws.close();
    }, []);

    return (
      <div>
        <div className="flex justify-between items-center mb-6">
          <h2 className="text-2xl font-bold">Live Payment Feed</h2>
          <div className={`px-4 py-2 rounded-full text-sm font-medium ${
            wsStatus === 'connected'
              ? 'bg-green-500 text-white'
              : 'bg-red-500 text-white'
          }`}>
            {wsStatus}
          </div>
        </div>

        <div className="space-y-4">
          {payments.length === 0 ? (
            <Card className="p-8 text-center text-gray-500">
              Waiting for payments...
            </Card>
          ) : (
            payments.map(payment => (
              <PaymentCard key={payment.event_id} payment={payment} />
            ))
          )}
        </div>
      </div>
    );
  }

  function PaymentCard({ payment }: { payment: PaymentEvent }) {
    const statusColors = {
      'x402.payment.requested': 'border-yellow-500 bg-yellow-50',
      'x402.payment.submitted': 'border-blue-500 bg-blue-50',
      'x402.payment.verified': 'border-green-500 bg-green-50',
      'x402.payment.failed': 'border-red-500 bg-red-50',
    };

    const statusIcons = {
      'x402.payment.requested': '⏳',
      'x402.payment.submitted': '📤',
      'x402.payment.verified': '✅',
      'x402.payment.failed': '❌',
    };

    return (
      <Card className={`p-4 border-l-4 ${statusColors[payment.event_type] || 'bg-gray-50'}`}>
        <div className="flex justify-between items-start">
          <div className="flex-1">
            <div className="flex items-center gap-2 mb-2">
              <span className="text-2xl">
                {statusIcons[payment.event_type] || '📝'}
              </span>
              <span className="font-mono text-sm text-gray-600">
                {payment.entity_id}
              </span>
            </div>

            <div className="font-bold text-lg">
              {payment.event_type.replace('x402.payment.', '').toUpperCase()}
            </div>

            {payment.data.signature && (
              <a
                href={`https://explorer.solana.com/tx/${payment.data.signature}?cluster=devnet`}
                target="_blank"
                rel="noopener noreferrer"
                className="text-blue-600 text-sm underline hover:text-blue-800"
              >
                View on Solana Explorer →
              </a>
            )}

            {payment.data.reason && (
              <div className="text-red-600 text-sm mt-1">
                Reason: {payment.data.reason}
              </div>
            )}
          </div>

          <div className="text-right">
            <div className="text-xs text-gray-500">
              {new Date(payment.timestamp).toLocaleTimeString()}
            </div>
            {payment.data.amount && (
              <div className="font-bold mt-1">
                ${(payment.data.amount / 1_000_000).toFixed(2)}
              </div>
            )}
          </div>
        </div>
      </Card>
    );
  }
  ```
- [ ] Add to dashboard page
- [ ] Test WebSocket connection
- [ ] Verify payments appear in real-time
- [ ] Test with multiple payment events
- [ ] Add error handling for disconnections

**Checkpoint at Hour 15:**
- [ ] Dashboard loads without errors?
- [ ] WebSocket connects successfully?
- [ ] Payments appear in real-time?
- [ ] UI is responsive on mobile?

---

### Hour 15-18: Analytics & Stats

#### Basic Metrics (1.5 hours)

- [ ] Create `components/dashboard/metrics.tsx`:
  ```typescript
  'use client';

  import { useEffect, useState } from 'react';
  import { Card } from '@/components/ui/card';

  export function DashboardMetrics() {
    const [stats, setStats] = useState({
      total: 0,
      verified: 0,
      failed: 0,
      totalVolume: 0
    });

    useEffect(() => {
      const fetchStats = async () => {
        const response = await fetch(
          `${process.env.NEXT_PUBLIC_ALLSOURCE_URL}/api/v1/events/query?event_type=x402.payment.*`
        );
        const data = await response.json();

        const total = data.events.length;
        const verified = data.events.filter(e =>
          e.event_type === 'x402.payment.verified'
        ).length;
        const failed = data.events.filter(e =>
          e.event_type === 'x402.payment.failed'
        ).length;
        const totalVolume = data.events
          .filter(e => e.event_type === 'x402.payment.verified')
          .reduce((sum, e) => sum + (e.data.amount || 0), 0);

        setStats({ total, verified, failed, totalVolume });
      };

      fetchStats();
      const interval = setInterval(fetchStats, 5000);
      return () => clearInterval(interval);
    }, []);

    return (
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
        <MetricCard
          title="Total Payments"
          value={stats.total}
          icon="💳"
        />
        <MetricCard
          title="Verified"
          value={stats.verified}
          icon="✅"
          color="text-green-600"
        />
        <MetricCard
          title="Failed"
          value={stats.failed}
          icon="❌"
          color="text-red-600"
        />
        <MetricCard
          title="Total Volume"
          value={`$${(stats.totalVolume / 1_000_000).toFixed(2)}`}
          icon="💰"
          color="text-blue-600"
        />
      </div>
    );
  }

  function MetricCard({ title, value, icon, color = 'text-gray-900' }) {
    return (
      <Card className="p-6">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm text-gray-600">{title}</div>
            <div className={`text-3xl font-bold ${color}`}>
              {value}
            </div>
          </div>
          <div className="text-4xl">{icon}</div>
        </div>
      </Card>
    );
  }
  ```
- [ ] Add to dashboard
- [ ] Test metrics update
- [ ] Verify calculations are correct

#### Success Rate Chart (1.5 hours)

- [ ] Install charting library: `bun add recharts`
- [ ] Create simple success rate visualization
- [ ] Add time-series data (if time permits)
- [ ] Make responsive

**Checkpoint at Hour 18:**
- [ ] Metrics display correctly?
- [ ] Charts render properly?
- [ ] Data updates in real-time?

---

### Hour 18-22: Time-Travel Feature 🌟

#### Time-Travel Component (3 hours)

- [ ] Create `components/dashboard/time-travel.tsx`:
  ```typescript
  'use client';

  import { useState } from 'react';
  import { Card } from '@/components/ui/card';
  import { Button } from '@/components/ui/button';

  export function TimeTravelDebugger({ paymentId }: { paymentId: string }) {
    const [timestamp, setTimestamp] = useState(() => {
      const date = new Date();
      date.setMinutes(date.getMinutes() - 5);
      return date.toISOString().slice(0, 16);
    });
    const [state, setState] = useState<any>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const handleTimeTravel = async () => {
      setLoading(true);
      setError(null);

      try {
        const isoTimestamp = new Date(timestamp).toISOString();
        const response = await fetch(
          `${process.env.NEXT_PUBLIC_ALLSOURCE_URL}/api/v1/entities/${paymentId}/state?as_of=${isoTimestamp}`
        );

        if (!response.ok) {
          throw new Error('Failed to reconstruct state');
        }

        const data = await response.json();
        setState(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Unknown error');
      } finally {
        setLoading(false);
      }
    };

    return (
      <Card className="p-6 mt-6 border-2 border-purple-500">
        <div className="flex items-center gap-3 mb-4">
          <span className="text-3xl">🕰️</span>
          <div>
            <h3 className="text-2xl font-bold">Time-Travel Debugger</h3>
            <p className="text-sm text-gray-600">
              Reconstruct exact payment state at any point in time
            </p>
          </div>
        </div>

        <div className="bg-purple-50 p-4 rounded-lg mb-4">
          <p className="text-sm">
            <strong>💡 What makes this special:</strong> Traditional databases
            lose historical state. With event sourcing, we can reconstruct the
            EXACT state of any payment at any point in time - perfect for
            disputes, audits, and compliance.
          </p>
        </div>

        <div className="flex gap-4 items-end mb-4">
          <div className="flex-1">
            <label className="block text-sm font-medium mb-2">
              Travel to timestamp:
            </label>
            <input
              type="datetime-local"
              value={timestamp}
              onChange={(e) => setTimestamp(e.target.value)}
              className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-purple-500"
            />
          </div>

          <Button
            onClick={handleTimeTravel}
            disabled={loading}
            className="px-6 py-2 bg-purple-600 hover:bg-purple-700"
          >
            {loading ? (
              <>⏳ Time Traveling...</>
            ) : (
              <>⚡ Time Travel</>
            )}
          </Button>
        </div>

        {error && (
          <div className="p-4 bg-red-100 text-red-700 rounded-lg mb-4">
            Error: {error}
          </div>
        )}

        {state && (
          <div className="mt-4">
            <h4 className="font-bold mb-2 flex items-center gap-2">
              <span>📸</span>
              Payment State at {new Date(timestamp).toLocaleString()}
            </h4>

            <pre className="bg-gray-900 text-green-400 p-4 rounded-lg overflow-auto font-mono text-sm max-h-96">
              {JSON.stringify(state, null, 2)}
            </pre>

            <div className="mt-4 text-sm text-gray-600">
              <p>
                This state reconstruction is powered by AllSource event store's
                time-travel queries - impossible with traditional databases!
              </p>
            </div>
          </div>
        )}
      </Card>
    );
  }
  ```
- [ ] Add toggle button to PaymentCard
- [ ] Test time-travel with different timestamps
- [ ] Verify state reconstruction works
- [ ] Add loading states
- [ ] Handle errors gracefully

#### Integration Testing (1 hour)

- [ ] Test complete flow:
  1. Make payment
  2. See event in feed
  3. Open time-travel debugger
  4. Select timestamp before payment
  5. Verify state shows no payment
  6. Select timestamp after payment
  7. Verify state shows payment details
- [ ] Test edge cases (future timestamp, very old timestamp)
- [ ] Verify error messages are helpful
- [ ] Test on mobile device

**Checkpoint at Hour 22:**
- [ ] Time-travel feature works?
- [ ] Can reconstruct historical state?
- [ ] UI is polished and intuitive?
- [ ] Feature is demo-ready?

---

### Hour 22-24: Final Polish & Submission

#### Deploy Dashboard (30 min)

- [ ] Deploy dashboard to Vercel:
  ```bash
  cd apps/x402-dashboard
  vercel --prod
  ```
- [ ] Set environment variables:
  - `NEXT_PUBLIC_ALLSOURCE_URL`
  - `NEXT_PUBLIC_ALLSOURCE_WS`
- [ ] Test deployed dashboard
- [ ] Verify WebSocket works in production
- [ ] Save URL: `_______________________________________________`

#### Demo Video Recording (1 hour)

- [ ] Set up recording (Loom/OBS in 1080p)
- [ ] Test audio levels
- [ ] Close unnecessary apps/tabs
- [ ] Clear browser cache/history
- [ ] Practice script once

**Recording Checklist:**
- [ ] Shot 1: Problem + code simplicity (0:00-0:30)
- [ ] Shot 2: Make payment via API (0:30-0:50)
- [ ] Shot 3: Dashboard real-time update (0:50-1:10)
- [ ] Shot 4: Time-travel demo (1:10-1:40) ⭐
- [ ] Shot 5: Wrap-up + CTA (1:40-2:00)

- [ ] Record video
- [ ] Review recording
- [ ] Re-record if needed
- [ ] Upload to YouTube (unlisted)
- [ ] Add to README
- [ ] Save URL: `_______________________________________________`

#### Documentation Finalization (30 min)

- [ ] Update main README.md:
  - Clear value proposition
  - Quick start (3 lines of code)
  - Live demo links
  - Video embed
  - Architecture diagram
  - Key features list
- [ ] Add GIF/screenshots to README
- [ ] Update package.json descriptions
- [ ] Add LICENSE file (MIT)
- [ ] Add CONTRIBUTING.md

#### Final Testing (30 min)

- [ ] Test all demo URLs work
- [ ] Verify video plays correctly
- [ ] Check README renders properly on GitHub
- [ ] Test on mobile device
- [ ] Ask someone else to try the demo
- [ ] Fix any obvious issues

#### Submission (30 min)

- [ ] Create GitHub repository (public)
- [ ] Push all code
- [ ] Verify repo looks good
- [ ] Submit to hackathon platform
- [ ] Include all required information:
  - Project name: "AllSource x402 - Solana Payment Infrastructure"
  - Description: Event-sourced payment infrastructure for x402
  - Demo URL: `_______________________________________________`
  - Video URL: `_______________________________________________`
  - GitHub URL: `_______________________________________________`
- [ ] Double-check submission is complete
- [ ] Take a screenshot of submission

**Final Checkpoint:**
- [ ] All code pushed to GitHub?
- [ ] Demo video uploaded and linked?
- [ ] All URLs working?
- [ ] Submission completed?
- [ ] Time-travel feature prominently featured?

---

## 🚨 EMERGENCY PROCEDURES

### If You're Running Out of Time

**Priority 1: Get SOMETHING submitted**
- [ ] Record demo video FIRST (even if features incomplete)
- [ ] Deploy what you have
- [ ] Write basic README
- [ ] Submit incomplete but functional demo

**Priority 2: Focus on differentiators**
- [ ] Time-travel feature > Everything else
- [ ] Working payment flow > Perfect UI
- [ ] Good video > Perfect code

**Priority 3: Cut scope**
- [ ] Skip advanced analytics
- [ ] Skip dashboard polish
- [ ] Skip comprehensive docs
- [ ] Skip multiple payment methods

### If Solana Devnet is Down

- [ ] Add mock mode to SDK:
  ```typescript
  const MOCK_MODE = process.env.MOCK_SOLANA === 'true';

  if (MOCK_MODE) {
    return { valid: true, amount: price * 1_000_000 };
  }
  ```
- [ ] Document in README that demo uses mock
- [ ] Show real Solana code in video
- [ ] Explain the mock in presentation

### If WebSocket Doesn't Work

- [ ] Implement polling fallback:
  ```typescript
  useEffect(() => {
    const interval = setInterval(async () => {
      const res = await fetch('/api/payments');
      const data = await res.json();
      setPayments(data);
    }, 3000);
    return () => clearInterval(interval);
  }, []);
  ```
- [ ] Update every 3 seconds instead
- [ ] Still demonstrates real-time capability

### If AllSource Deployment Fails

- [ ] Use local AllSource instance
- [ ] Record demo video with localhost
- [ ] Deploy to Railway as backup
- [ ] Contact for help in community

---

## ✅ QUALITY GATES

### Before Moving to Next Phase

**After Pre-Hackathon:**
- [ ] ALL infrastructure deployed and tested
- [ ] Can ingest events to AllSource
- [ ] Can verify Solana transactions
- [ ] Build system works end-to-end

**After Day 1:**
- [ ] SDK compiles without errors
- [ ] Demo API returns 402
- [ ] Can process paid requests
- [ ] Events logged to AllSource
- [ ] Deployed to production

**After Day 2:**
- [ ] Dashboard shows real-time data
- [ ] Time-travel feature works
- [ ] Demo video recorded
- [ ] Documentation complete
- [ ] Ready to submit

---

## 📊 PROGRESS TRACKING

### Overall Status

**Pre-Hackathon:** [ ] Not Started [ ] In Progress [ ] Complete

**Day 1 Morning (Hour 0-6):** [ ] Not Started [ ] In Progress [ ] Complete

**Day 1 Afternoon (Hour 6-12):** [ ] Not Started [ ] In Progress [ ] Complete

**Day 2 Morning (Hour 12-18):** [ ] Not Started [ ] In Progress [ ] Complete

**Day 2 Afternoon (Hour 18-24):** [ ] Not Started [ ] In Progress [ ] Complete

### Key Deliverables

- [ ] AllSource deployed and accessible
- [ ] SDK package published
- [ ] Demo API deployed
- [ ] Dashboard deployed
- [ ] Time-travel feature working
- [ ] Demo video completed
- [ ] Documentation finished
- [ ] Hackathon submission complete

### Risk Status

🟢 **Green:** On track, no issues
🟡 **Yellow:** Minor issues, manageable
🔴 **Red:** Blocked, need help

Current Status: _____ (Update throughout hackathon)

---

## 🎯 SUCCESS CRITERIA

### Must Have (Required)
- [x] Working payment flow (402 → verify → process)
- [x] Events logged to AllSource
- [x] Demo video (2 min max)
- [x] Time-travel feature functional
- [x] Deployed demo accessible

### Should Have (Competitive)
- [ ] Real-time dashboard
- [ ] Clean UI design
- [ ] Good documentation
- [ ] Mobile responsive
- [ ] Analytics metrics

### Nice to Have (Bonus)
- [ ] Fraud detection
- [ ] Advanced charts
- [ ] Multiple demo examples
- [ ] Comprehensive tests

---

## 📝 NOTES & LEARNINGS

**Issues Encountered:**
```
[Date/Time] - Issue:
Solution:

[Date/Time] - Issue:
Solution:
```

**Optimizations Made:**
```
[Date/Time] - Optimization:
Impact:
```

**Things to Remember for Next Time:**
```
1.
2.
3.
```

---

## 🎉 POST-HACKATHON

After submission:
- [ ] Rest! 😴
- [ ] Share on social media
- [ ] Engage with hackathon community
- [ ] Prepare for demo day presentation
- [ ] Gather feedback
- [ ] Plan next steps for the project

---

**Last Updated:** [Date]
**Current Phase:** [Phase Name]
**Completion:** ____%
**Next Milestone:** [Next Task]

---

**Remember:**
- ⏰ Time-travel feature is your competitive advantage
- 📹 Demo video is critical - record early
- 🚀 Shipping > Perfection
- 🛟 Ask for help if blocked > 30 minutes
- 🎯 Focus on the wow factor

**Good luck! You've got this! 🚀**
