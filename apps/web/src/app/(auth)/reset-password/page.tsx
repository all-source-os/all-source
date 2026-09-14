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
import { AlertCircle, CheckCircle2, Eye, EyeOff, Loader2, XCircle } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { Suspense, useEffect, useState } from "react";

const PASSWORD_REQUIREMENTS = [
  { regex: /.{8,}/, label: "At least 8 characters" },
  { regex: /[A-Z]/, label: "One uppercase letter" },
  { regex: /[a-z]/, label: "One lowercase letter" },
  { regex: /[0-9]/, label: "One number" },
];

type ResetState = "form" | "success" | "error" | "expired";

function ResetPasswordContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [state, setState] = useState<ResetState>("form");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const token = searchParams.get("token");

  useEffect(() => {
    if (!token) {
      setState("error");
      setError("Missing reset token. Please request a new password reset link.");
    }
  }, [token]);

  const getPasswordStrength = () => {
    return PASSWORD_REQUIREMENTS.filter((req) => req.regex.test(password)).length;
  };

  const isPasswordValid = getPasswordStrength() === PASSWORD_REQUIREMENTS.length;
  const passwordsMatch = password === confirmPassword && confirmPassword.length > 0;
  const isFormValid = isPasswordValid && passwordsMatch;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isFormValid || !token) return;

    setError(null);
    setIsSubmitting(true);

    try {
      const response = await fetch("/api/auth/reset-password", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ token, password }),
      });

      const data = await response.json();

      if (!response.ok) {
        if (response.status === 410 || data.error?.code === "token_expired") {
          setState("expired");
        } else {
          setError(data.error?.message || data.message || "Password reset failed.");
        }
        return;
      }

      setState("success");

      // Auto-redirect to login after 3 seconds
      setTimeout(() => {
        router.push("/login?reset=true");
      }, 3000);
    } catch {
      setError("Failed to reset password. Please try again.");
    } finally {
      setIsSubmitting(false);
    }
  };

  if (state === "success") {
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
                <h2 className="mb-2 text-xl font-semibold">Password reset successful</h2>
                <p className="mb-6 text-muted-foreground">
                  Your password has been reset. You'll be redirected to sign in shortly.
                </p>
                <Button onClick={() => router.push("/login")} className="w-full">
                  Continue to Sign In
                </Button>
              </CardContent>
            </Card>
          </BlurFade>
        </div>
      </div>
    );
  }

  if (state === "expired") {
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
                <div className="mb-4 rounded-full bg-yellow-500/10 p-3">
                  <XCircle className="h-8 w-8 text-yellow-600" />
                </div>
                <h2 className="mb-2 text-xl font-semibold">Link expired</h2>
                <p className="mb-6 text-muted-foreground">
                  This password reset link has expired. Please request a new one.
                </p>
                <div className="space-y-3 w-full">
                  <Link href="/forgot-password" className="block">
                    <Button className="w-full">Request new reset link</Button>
                  </Link>
                  <Link href="/login" className="block">
                    <Button variant="ghost" className="w-full">
                      Back to Sign In
                    </Button>
                  </Link>
                </div>
              </CardContent>
            </Card>
          </BlurFade>
        </div>
      </div>
    );
  }

  if (state === "error" && !token) {
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
                <div className="mb-4 rounded-full bg-destructive/10 p-3">
                  <XCircle className="h-8 w-8 text-destructive" />
                </div>
                <h2 className="mb-2 text-xl font-semibold">Invalid link</h2>
                <p className="mb-6 text-muted-foreground">{error}</p>
                <div className="space-y-3 w-full">
                  <Link href="/forgot-password" className="block">
                    <Button className="w-full">Request new reset link</Button>
                  </Link>
                  <Link href="/login" className="block">
                    <Button variant="ghost" className="w-full">
                      Back to Sign In
                    </Button>
                  </Link>
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
              <CardTitle className="text-2xl font-semibold">Set new password</CardTitle>
              <CardDescription className="text-base">
                Create a strong password for your account
              </CardDescription>
            </CardHeader>
            <CardContent className="px-6 pb-6 pt-6 sm:px-8 sm:pb-8">
              {error && (
                <div
                  role="alert"
                  className="mb-5 flex items-start gap-2 rounded-lg bg-destructive/10 px-4 py-3 text-sm text-destructive"
                >
                  <AlertCircle className="h-4 w-4 mt-0.5 shrink-0" />
                  {error}
                </div>
              )}

              <form onSubmit={handleSubmit} className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="password">New password</Label>
                  <div className="relative">
                    <Input
                      id="password"
                      type={showPassword ? "text" : "password"}
                      placeholder="Create a password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                      disabled={isSubmitting}
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
                      {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                  </div>

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

                <div className="space-y-2">
                  <Label htmlFor="confirmPassword">Confirm password</Label>
                  <div className="relative">
                    <Input
                      id="confirmPassword"
                      type={showConfirmPassword ? "text" : "password"}
                      placeholder="Confirm your password"
                      value={confirmPassword}
                      onChange={(e) => setConfirmPassword(e.target.value)}
                      disabled={isSubmitting}
                      autoComplete="new-password"
                      className="pr-10"
                      required
                    />
                    <button
                      type="button"
                      onClick={() => setShowConfirmPassword(!showConfirmPassword)}
                      className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                      tabIndex={-1}
                    >
                      {showConfirmPassword ? (
                        <EyeOff className="h-4 w-4" />
                      ) : (
                        <Eye className="h-4 w-4" />
                      )}
                    </button>
                  </div>

                  {confirmPassword && !passwordsMatch && (
                    <p className="text-xs text-destructive">Passwords do not match</p>
                  )}
                  {passwordsMatch && (
                    <p className="flex items-center gap-1 text-xs text-green-600">
                      <CheckCircle2 className="h-3 w-3" />
                      Passwords match
                    </p>
                  )}
                </div>

                <Button
                  type="submit"
                  className="h-12 w-full"
                  disabled={isSubmitting || !isFormValid}
                >
                  {isSubmitting ? <Loader2 className="h-5 w-5 animate-spin" /> : "Reset password"}
                </Button>
              </form>
            </CardContent>
          </Card>
        </BlurFade>
      </div>
    </div>
  );
}

function ResetPasswordLoading() {
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
            <CardTitle className="text-2xl font-semibold">Set new password</CardTitle>
            <CardDescription className="text-base">
              Create a strong password for your account
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

export default function ResetPasswordPage() {
  return (
    <Suspense fallback={<ResetPasswordLoading />}>
      <ResetPasswordContent />
    </Suspense>
  );
}
