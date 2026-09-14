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
import { AlertCircle, Eye, EyeOff, Loader2, Mail } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useEffect, useId, useState } from "react";

const ERROR_MESSAGES: Record<string, string> = {
  missing_token: "Authentication failed. Please try again.",
  invalid_token: "Session expired. Please sign in again.",
  auth_failed: "Authentication failed. Please try again.",
  access_denied: "Access was denied. Please try again.",
  invalid_credentials: "Invalid email or password.",
  email_not_verified: "Please verify your email before signing in.",
  account_locked: "Your account has been locked. Please contact support.",
};

function LoginContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [loadingProvider, setLoadingProvider] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showEmailForm, setShowEmailForm] = useState(false);
  const [showPassword, setShowPassword] = useState(false);

  // Form state
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const errorId = useId();

  // Check for OAuth errors in URL
  useEffect(() => {
    const errorParam = searchParams.get("error");
    if (errorParam) {
      setError(ERROR_MESSAGES[errorParam] || "An error occurred. Please try again.");
    }
  }, [searchParams]);

  const handleOAuthLogin = (provider: "google" | "github") => {
    setLoadingProvider(provider);
    setError(null);
    // Use same-origin path — Next.js rewrites proxy this to the control plane
    window.location.href = `/api/v1/auth/oauth/${provider}`;
  };

  const handleEmailLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setIsSubmitting(true);

    try {
      const response = await fetch("/api/v1/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password }),
      });

      const data = await response.json();

      if (!response.ok) {
        throw new Error(data.error?.message || data.message || "Login failed");
      }

      // Route through the callback handler to set the httpOnly auth cookie.
      // Forward ?next= so deep-links like /connect resume after sign-in.
      if (data.token) {
        const isNewUser = data.new_user === true;
        const next = searchParams.get("next");
        const nextParam = next ? `&next=${encodeURIComponent(next)}` : "";
        window.location.href = `/api/auth/callback?token=${encodeURIComponent(data.token)}&new_user=${isNewUser}${nextParam}`;
      } else {
        router.push(searchParams.get("next") ?? "/dashboard");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed. Please try again.");
    } finally {
      setIsSubmitting(false);
    }
  };

  const isFormValid = email.trim() && password.trim();
  const isDisabled = loadingProvider !== null || isSubmitting;

  return (
    <div className="relative min-h-screen w-full overflow-hidden">
      {/* Background pattern */}
      <DotPattern
        className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
        cr={1}
        cx={1}
        cy={1}
      />

      {/* Content */}
      <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 sm:px-6">
        <BlurFade delay={0.1} inView>
          {/* Logo and branding */}
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
              <CardTitle className="text-2xl font-semibold">Welcome back</CardTitle>
              <CardDescription className="text-base">
                Sign in to your account to continue
              </CardDescription>
            </CardHeader>
            <CardContent className="px-6 pb-6 pt-6 sm:px-8 sm:pb-8">
              {/* Error message */}
              {error && (
                <div
                  id={errorId}
                  role="alert"
                  className="mb-5 flex items-start gap-2 rounded-lg bg-destructive/10 px-4 py-3 text-sm text-destructive"
                >
                  <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
                  {error}
                </div>
              )}

              {showEmailForm ? (
                // Email login form
                <form onSubmit={handleEmailLogin} className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="email">Email address</Label>
                    <Input
                      id="email"
                      type="email"
                      placeholder="you@example.com"
                      value={email}
                      onChange={(e) => setEmail(e.target.value)}
                      disabled={isDisabled}
                      autoComplete="email"
                      autoFocus
                      required
                    />
                  </div>

                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <Label htmlFor="password">Password</Label>
                      <Link
                        href="/forgot-password"
                        className="text-xs text-muted-foreground hover:text-primary"
                      >
                        Forgot password?
                      </Link>
                    </div>
                    <div className="relative">
                      <Input
                        id="password"
                        type={showPassword ? "text" : "password"}
                        placeholder="Enter your password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        disabled={isDisabled}
                        autoComplete="current-password"
                        className="pr-10"
                        required
                      />
                      <button
                        type="button"
                        onClick={() => setShowPassword(!showPassword)}
                        className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                        tabIndex={-1}
                      >
                        {showPassword ? (
                          <EyeOff className="h-4 w-4" />
                        ) : (
                          <Eye className="h-4 w-4" />
                        )}
                      </button>
                    </div>
                  </div>

                  <Button
                    type="submit"
                    className="h-12 w-full"
                    disabled={isDisabled || !isFormValid}
                  >
                    {isSubmitting ? <Loader2 className="h-5 w-5 animate-spin" /> : "Sign in"}
                  </Button>

                  <button
                    type="button"
                    onClick={() => setShowEmailForm(false)}
                    className="w-full text-center text-sm text-muted-foreground hover:text-foreground"
                  >
                    ← Back to all options
                  </button>
                </form>
              ) : (
                <>
                  {/* OAuth buttons */}
                  <div className="grid gap-3">
                    <Button
                      type="button"
                      variant="outline"
                      className="relative h-12 w-full bg-white hover:bg-gray-50 dark:bg-zinc-900 dark:hover:bg-zinc-800"
                      onClick={() => handleOAuthLogin("google")}
                      disabled={isDisabled}
                      aria-busy={loadingProvider === "google"}
                    >
                      {loadingProvider === "google" ? (
                        <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />
                      ) : (
                        <>
                          <Icons.google className="mr-2.5 h-5 w-5" aria-hidden="true" />
                          Continue with Google
                        </>
                      )}
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      className="h-12 w-full bg-zinc-900 text-white hover:bg-zinc-800 dark:bg-white dark:text-zinc-900 dark:hover:bg-zinc-100"
                      onClick={() => handleOAuthLogin("github")}
                      disabled={isDisabled}
                      aria-busy={loadingProvider === "github"}
                    >
                      {loadingProvider === "github" ? (
                        <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />
                      ) : (
                        <>
                          <Icons.github className="mr-2.5 h-5 w-5" aria-hidden="true" />
                          Continue with GitHub
                        </>
                      )}
                    </Button>
                  </div>

                  {/* Divider */}
                  <div className="relative my-6">
                    <div className="absolute inset-0 flex items-center">
                      <span className="w-full border-t border-border" />
                    </div>
                    <div className="relative flex justify-center text-xs uppercase">
                      <span className="bg-background px-2 text-muted-foreground">
                        Or continue with
                      </span>
                    </div>
                  </div>

                  {/* Email login button */}
                  <Button
                    type="button"
                    variant="outline"
                    className="h-12 w-full"
                    onClick={() => setShowEmailForm(true)}
                    disabled={isDisabled}
                  >
                    <Mail className="mr-2.5 h-5 w-5" />
                    Continue with Email
                  </Button>
                </>
              )}

              {/* Sign up link */}
              <p className="mt-6 text-center text-sm text-muted-foreground">
                Don&apos;t have an account?{" "}
                <Link
                  href="/signup"
                  className="font-medium text-primary underline-offset-4 transition-colors hover:underline focus:outline-none focus-visible:underline"
                >
                  Create one
                </Link>
              </p>
            </CardContent>
          </Card>
        </BlurFade>

        {/* Footer */}
        <BlurFade delay={0.3} inView>
          <p className="mt-10 text-center text-xs text-muted-foreground">
            By continuing, you agree to our{" "}
            <Link
              href="/terms"
              className="underline underline-offset-4 transition-colors hover:text-foreground focus:outline-none focus-visible:text-foreground"
            >
              Terms of Service
            </Link>{" "}
            and{" "}
            <Link
              href="/privacy"
              className="underline underline-offset-4 transition-colors hover:text-foreground focus:outline-none focus-visible:text-foreground"
            >
              Privacy Policy
            </Link>
          </p>
        </BlurFade>
      </div>
    </div>
  );
}

function LoginLoading() {
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
            <CardTitle className="text-2xl font-semibold">Welcome back</CardTitle>
            <CardDescription className="text-base">
              Sign in to your account to continue
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

export default function LoginPage() {
  return (
    <Suspense fallback={<LoginLoading />}>
      <LoginContent />
    </Suspense>
  );
}
