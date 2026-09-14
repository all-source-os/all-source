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
import { cn } from "@allsource/ui/utils";
import { Turnstile, type TurnstileInstance } from "@marsidev/react-turnstile";
import { AlertCircle, CheckCircle2, Eye, EyeOff, Loader2, Mail } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useCallback, useEffect, useId, useRef, useState } from "react";
import { reportGeoConversion } from "@/components/geo-referral-tracker";

const TURNSTILE_SITE_KEY = process.env.NEXT_PUBLIC_TURNSTILE_SITE_KEY || "";

const ERROR_MESSAGES: Record<string, string> = {
  missing_token: "Authentication failed. Please try again.",
  invalid_token: "Session expired. Please sign in again.",
  auth_failed: "Authentication failed. Please try again.",
  access_denied: "Access was denied. Please try again.",
  email_exists: "An account with this email already exists.",
  invalid_email: "Please enter a valid email address.",
  weak_password: "Password must be at least 8 characters.",
};

const PASSWORD_REQUIREMENTS = [
  { regex: /.{8,}/, label: "At least 8 characters" },
  { regex: /[A-Z]/, label: "One uppercase letter" },
  { regex: /[a-z]/, label: "One lowercase letter" },
  { regex: /[0-9]/, label: "One number" },
];

function SignUpContent() {
  const _router = useRouter();
  const searchParams = useSearchParams();
  const [loadingProvider, setLoadingProvider] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showEmailForm, setShowEmailForm] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [emailSent, setEmailSent] = useState(false);

  // Form state
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [turnstileToken, setTurnstileToken] = useState<string | null>(null);
  const turnstileRef = useRef<TurnstileInstance>(null);

  const handleTurnstileSuccess = useCallback((token: string) => {
    setTurnstileToken(token);
  }, []);

  const errorId = useId();

  // Check for OAuth errors and prefilled email in URL
  useEffect(() => {
    const errorParam = searchParams.get("error");
    if (errorParam) {
      setError(ERROR_MESSAGES[errorParam] || "An error occurred. Please try again.");
    }
    const emailParam = searchParams.get("email");
    if (emailParam) {
      setEmail(emailParam);
      setShowEmailForm(true);
    }
  }, [searchParams]);

  const handleOAuthSignUp = (provider: "google" | "github") => {
    // GEO layer 1: if this session arrived from an AI surface, mark it
    // converted. A no-op for every other session, and it never blocks signup.
    reportGeoConversion("signup_started");
    setLoadingProvider(provider);
    setError(null);
    // Use same-origin path — Next.js rewrites proxy this to the control plane
    window.location.href = `/api/v1/auth/oauth/${provider}`;
  };

  const handleEmailSignUp = async (e: React.FormEvent) => {
    e.preventDefault();
    reportGeoConversion("signup_started");
    setError(null);
    setIsSubmitting(true);

    try {
      const response = await fetch("/api/v1/auth/register", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name,
          email,
          password,
          ...(turnstileToken && { cf_turnstile_response: turnstileToken }),
        }),
      });

      const data = await response.json();

      if (!response.ok) {
        throw new Error(data.error?.message || data.message || "Registration failed");
      }

      // Route through the callback handler to set the httpOnly auth cookie.
      // Forward ?next= so deep-links like /connect resume after signup.
      if (data.token) {
        const isNewUser = data.new_user === true;
        const next = searchParams.get("next");
        const nextParam = next ? `&next=${encodeURIComponent(next)}` : "";
        window.location.href = `/api/auth/callback?token=${encodeURIComponent(data.token)}&new_user=${isNewUser}${nextParam}`;
      } else {
        // Fallback: show email verification message
        setEmailSent(true);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Registration failed. Please try again.");
      turnstileRef.current?.reset();
    } finally {
      setIsSubmitting(false);
    }
  };

  const getPasswordStrength = () => {
    return PASSWORD_REQUIREMENTS.filter((req) => req.regex.test(password)).length;
  };

  const isPasswordValid = getPasswordStrength() === PASSWORD_REQUIREMENTS.length;
  const isFormValid = name.trim() && email.trim() && isPasswordValid;
  const isDisabled = loadingProvider !== null || isSubmitting;

  if (emailSent) {
    return (
      <div className="relative min-h-screen w-full overflow-hidden">
        <DotPattern
          className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
          cr={1}
          cx={1}
          cy={1}
        />
        <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 py-10 sm:px-6">
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
                  <Mail className="h-8 w-8 text-green-500" />
                </div>
                <h2 className="mb-2 text-xl font-semibold">Check your email</h2>
                <p className="mb-6 text-muted-foreground">
                  We've sent a verification link to <strong>{email}</strong>. Click the link to
                  activate your account.
                </p>
                <div className="space-y-3 w-full">
                  <Button variant="outline" className="w-full" onClick={() => setEmailSent(false)}>
                    Use a different email
                  </Button>
                  <p className="text-xs text-muted-foreground">
                    Didn't receive the email? Check your spam folder or{" "}
                    <button
                      type="button"
                      className="text-primary underline-offset-4 hover:underline"
                      onClick={handleEmailSignUp}
                      disabled={isSubmitting}
                    >
                      resend
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
      {/* Background pattern */}
      <DotPattern
        className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
        cr={1}
        cx={1}
        cy={1}
      />

      {/* Content */}
      <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 py-10 sm:px-6">
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
              <CardTitle className="text-2xl font-semibold">Create an account</CardTitle>
              <CardDescription className="text-base">
                Start your 14-day AllSource trial
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
                // Email signup form
                <form onSubmit={handleEmailSignUp} className="space-y-4">
                  <div className="space-y-2">
                    <Label htmlFor="name">Full name</Label>
                    <Input
                      id="name"
                      type="text"
                      placeholder="John Doe"
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                      disabled={isDisabled}
                      autoComplete="name"
                      required
                    />
                  </div>

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
                      required
                    />
                  </div>

                  <div className="space-y-2">
                    <Label htmlFor="password">Password</Label>
                    <div className="relative">
                      <Input
                        id="password"
                        type={showPassword ? "text" : "password"}
                        placeholder="Create a password"
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        disabled={isDisabled}
                        autoComplete="new-password"
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

                    {/* Password requirements */}
                    {password && (
                      <div className="mt-2 space-y-1">
                        {PASSWORD_REQUIREMENTS.map((req) => (
                          <div
                            key={req.label}
                            className={cn(
                              "flex items-center gap-2 text-xs",
                              req.regex.test(password) ? "text-green-600" : "text-muted-foreground"
                            )}
                          >
                            <CheckCircle2
                              className={cn(
                                "h-3 w-3",
                                req.regex.test(password) ? "opacity-100" : "opacity-40"
                              )}
                            />
                            {req.label}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  {TURNSTILE_SITE_KEY && (
                    <Turnstile
                      ref={turnstileRef}
                      siteKey={TURNSTILE_SITE_KEY}
                      onSuccess={handleTurnstileSuccess}
                      options={{ size: "invisible", theme: "dark" }}
                    />
                  )}

                  <Button
                    type="submit"
                    className="h-12 w-full"
                    disabled={isDisabled || !isFormValid}
                  >
                    {isSubmitting ? <Loader2 className="h-5 w-5 animate-spin" /> : "Create account"}
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
                      onClick={() => handleOAuthSignUp("google")}
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
                      onClick={() => handleOAuthSignUp("github")}
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

                  {/* Email signup button */}
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

                  {/* Features highlight */}
                  <div className="mt-6 space-y-3 rounded-lg bg-muted/50 p-4">
                    <p className="text-sm font-medium">Your 14-day trial includes:</p>
                    <ul className="space-y-2 text-sm text-muted-foreground">
                      <li className="flex items-center gap-2">
                        <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                        Full hosted event store
                      </li>
                      <li className="flex items-center gap-2">
                        <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                        Real-time streaming
                      </li>
                      <li className="flex items-center gap-2">
                        <span className="h-1.5 w-1.5 rounded-full bg-primary" />
                        AI-powered analytics
                      </li>
                    </ul>
                  </div>
                </>
              )}

              {/* Sign in link */}
              <p className="mt-6 text-center text-sm text-muted-foreground">
                Already have an account?{" "}
                <Link
                  href="/login"
                  className="font-medium text-primary underline-offset-4 transition-colors hover:underline focus:outline-none focus-visible:underline"
                >
                  Sign in
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

function SignUpLoading() {
  return (
    <div className="relative min-h-screen w-full overflow-hidden">
      <DotPattern
        className="opacity-50 dark:opacity-30 [mask-image:radial-gradient(ellipse_at_center,transparent_20%,black)]"
        cr={1}
        cx={1}
        cy={1}
      />
      <div className="relative z-10 flex min-h-screen flex-col items-center justify-center px-4 py-10 sm:px-6">
        <div className="mb-10 flex flex-col items-center gap-3">
          <div className="flex items-center gap-2.5">
            <Icons.logo className="h-10 w-10 text-primary" />
            <span className="text-3xl font-bold tracking-tight">AllSource</span>
          </div>
          <p className="text-muted-foreground">Purpose-built event store database</p>
        </div>
        <Card className="w-full max-w-[420px] border-border/50 bg-background/80 px-2 py-2 backdrop-blur-sm sm:px-4 sm:py-4">
          <CardHeader className="space-y-2 px-6 pb-0 pt-4 text-center sm:px-8 sm:pt-6">
            <CardTitle className="text-2xl font-semibold">Create an account</CardTitle>
            <CardDescription className="text-base">
              Start your 14-day AllSource trial
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

export default function SignUpPage() {
  return (
    <Suspense fallback={<SignUpLoading />}>
      <SignUpContent />
    </Suspense>
  );
}
