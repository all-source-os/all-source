"use client";

import {
  BlurFade,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  DotPattern,
  Icons,
  Input,
  Label,
} from "@allsource/ui";
import { ArrowLeft, CheckCircle2, Loader2, Mail } from "lucide-react";
import Link from "next/link";
import { Suspense, useState } from "react";

function ForgotPasswordContent() {
  const [email, setEmail] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isSuccess, setIsSuccess] = useState(false);
  const [_error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setIsSubmitting(true);

    try {
      const _response = await fetch("/api/auth/forgot-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email }),
      });

      // Always show success to prevent email enumeration
      setIsSuccess(true);
    } catch {
      // Still show success for security
      setIsSuccess(true);
    } finally {
      setIsSubmitting(false);
    }
  };

  if (isSuccess) {
    return (
      <div className="relative min-h-screen w-full overflow-hidden">
        <DotPattern
          className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
          cr={1}
          cx={1}
          cy={1}
        />
        <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 sm:px-6">
          <BlurFade delay={0.1} inView>
            <div className="mb-10 flex flex-col items-center gap-3">
              <div className="flex items-center gap-2.5">
                <Icons.logo className="h-10 w-10 text-primary" />
                <span className="text-3xl font-bold tracking-tight">AllSource</span>
              </div>
            </div>
          </BlurFade>

          <BlurFade delay={0.2} inView>
            <Card className="w-full max-w-[420px] border-border/50 bg-background/80 px-2 py-2 backdrop-blur-sm sm:px-4 sm:py-4">
              <CardContent className="flex flex-col items-center px-6 py-8 text-center sm:px-8">
                <div className="mb-4 rounded-full bg-green-500/10 p-3">
                  <CheckCircle2 className="h-8 w-8 text-green-500" />
                </div>
                <h2 className="mb-2 text-xl font-semibold">Check your email</h2>
                <p className="mb-6 text-muted-foreground">
                  If an account exists for <strong>{email}</strong>, we've sent a password reset
                  link.
                </p>
                <div className="space-y-3 w-full">
                  <Link href="/login" className="block">
                    <Button variant="outline" className="w-full">
                      <ArrowLeft className="mr-2 h-4 w-4" />
                      Back to Sign In
                    </Button>
                  </Link>
                  <p className="text-xs text-muted-foreground">
                    Didn't receive the email? Check your spam folder or{" "}
                    <button
                      type="button"
                      className="text-primary underline-offset-4 hover:underline"
                      onClick={() => {
                        setIsSuccess(false);
                        setEmail("");
                      }}
                    >
                      try again
                    </button>
                  </p>
                </div>
              </CardContent>
            </Card>
          </BlurFade>
        </div>
      </div>
    );
  }

  return (
    <div className="relative min-h-screen w-full overflow-hidden">
      <DotPattern
        className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
        cr={1}
        cx={1}
        cy={1}
      />

      <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 sm:px-6">
        <BlurFade delay={0.1} inView>
          <div className="mb-10 flex flex-col items-center gap-3">
            <div className="flex items-center gap-2.5">
              <Icons.logo className="h-10 w-10 text-primary" />
              <span className="text-3xl font-bold tracking-tight">AllSource</span>
            </div>
            <p className="text-muted-foreground">Purpose-built event store database</p>
          </div>
        </BlurFade>

        <BlurFade delay={0.2} inView>
          <Card className="w-full max-w-[420px] border-border/50 bg-background/80 px-2 py-2 backdrop-blur-sm sm:px-4 sm:py-4">
            <CardHeader className="space-y-2 px-6 pb-0 pt-4 text-center sm:px-8 sm:pt-6">
              <CardTitle className="text-2xl font-semibold">Reset your password</CardTitle>
              <CardDescription className="text-base">
                Enter your email and we'll send you a reset link
              </CardDescription>
            </CardHeader>
            <CardContent className="px-6 pb-6 pt-6 sm:px-8 sm:pb-8">
              <form onSubmit={handleSubmit} className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="email">Email address</Label>
                  <Input
                    id="email"
                    type="email"
                    placeholder="you@example.com"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    disabled={isSubmitting}
                    autoComplete="email"
                    autoFocus
                    required
                  />
                </div>

                <Button
                  type="submit"
                  className="h-12 w-full"
                  disabled={isSubmitting || !email.trim()}
                >
                  {isSubmitting ? (
                    <Loader2 className="h-5 w-5 animate-spin" />
                  ) : (
                    <>
                      <Mail className="mr-2 h-5 w-5" />
                      Send reset link
                    </>
                  )}
                </Button>

                <Link
                  href="/login"
                  className="flex items-center justify-center gap-2 text-sm text-muted-foreground hover:text-foreground"
                >
                  <ArrowLeft className="h-4 w-4" />
                  Back to Sign In
                </Link>
              </form>
            </CardContent>
          </Card>
        </BlurFade>
      </div>
    </div>
  );
}

function ForgotPasswordLoading() {
  return (
    <div className="relative min-h-screen w-full overflow-hidden">
      <DotPattern
        className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
        cr={1}
        cx={1}
        cy={1}
      />
      <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 sm:px-6">
        <div className="mb-10 flex flex-col items-center gap-3">
          <div className="flex items-center gap-2.5">
            <Icons.logo className="h-10 w-10 text-primary" />
            <span className="text-3xl font-bold tracking-tight">AllSource</span>
          </div>
          <p className="text-muted-foreground">Purpose-built event store database</p>
        </div>
        <Card className="w-full max-w-[420px] border-border/50 bg-background/80 px-2 py-2 backdrop-blur-sm sm:px-4 sm:py-4">
          <CardHeader className="space-y-2 px-6 pb-0 pt-4 text-center sm:px-8 sm:pt-6">
            <CardTitle className="text-2xl font-semibold">Reset your password</CardTitle>
            <CardDescription className="text-base">
              Enter your email and we'll send you a reset link
            </CardDescription>
          </CardHeader>
          <CardContent className="flex items-center justify-center px-6 pb-6 pt-6 sm:px-8 sm:pb-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

export default function ForgotPasswordPage() {
  return (
    <Suspense fallback={<ForgotPasswordLoading />}>
      <ForgotPasswordContent />
    </Suspense>
  );
}
