import { useState } from "react";
import type { FormEvent } from "react";
import { useNavigate } from "react-router";
import { Server } from "lucide-react";
import { ApiClientError, post } from "../api/client";
import { setToken } from "./token";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface LoginResponse {
  token: string;
  pid: string;
  name: string;
  is_verified: boolean;
}

// Dev auto-fill: `vite.config.ts` injects IPXE_ZTP_TEST_EMAIL/IPXE_ZTP_TEST_PASSWORD from .env
// into the bundle when the dev server runs with IPXE_ZTP_IS_TEST=true, so the login
// form starts pre-filled in development. Production builds compile these to
// empty strings — the password is never baked into a released bundle.
const AUTO_FILL = import.meta.env.VITE_IPXE_ZTP_IS_TEST === "true";
const AUTO_EMAIL = AUTO_FILL ? (import.meta.env.VITE_IPXE_ZTP_TEST_EMAIL || "") : "";
const AUTO_PASSWORD = AUTO_FILL ? (import.meta.env.VITE_IPXE_ZTP_TEST_PASSWORD || "") : "";

export function Login() {
  const navigate = useNavigate();
  const [email, setEmail] = useState(AUTO_EMAIL);
  const [password, setPassword] = useState(AUTO_PASSWORD);
  const [isPending, setIsPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setError(null);
    setIsPending(true);
    try {
      const res = await post<LoginResponse>("/api/auth/login", {
        email,
        password,
      });
      setToken(res.token);
      navigate("/");
    } catch (err) {
      setError(
        err instanceof ApiClientError ? err.message : "Failed to log in",
      );
    } finally {
      setIsPending(false);
    }
  }

  return (
    <div className="relative flex min-h-svh items-center justify-center overflow-hidden bg-neutral-950 p-6">
      {/* Dot grid pattern */}
      <div
        aria-hidden
        className="absolute inset-0 bg-[radial-gradient(circle_at_1px_1px,rgba(255,255,255,0.09)_1px,transparent_0)] bg-[size:22px_22px]"
      />
      {/* Radial glow */}
      <div
        aria-hidden
        className="absolute inset-0 bg-[radial-gradient(ellipse_60%_50%_at_50%_40%,rgba(255,255,255,0.10),transparent_70%)]"
      />
      {/* Fade edges so the card area reads cleanly */}
      <div
        aria-hidden
        className="absolute inset-0 bg-[radial-gradient(ellipse_50%_45%_at_50%_50%,rgba(10,10,10,0.85),transparent_75%)]"
      />

      <Card className="relative w-full max-w-sm shadow-2xl shadow-black/50">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Server className="size-5" />
            iPXE ZTP Server
          </CardTitle>
          <CardDescription>
            Sign in to manage machines, OS versions, and zero-touch network
            provisioning.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="email">Email</Label>
              <Input
                id="email"
                name="email"
                type="email"
                autoComplete="email"
                placeholder="you@example.com"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="password">Password</Label>
              <Input
                id="password"
                name="password"
                type="password"
                autoComplete="current-password"
                placeholder="••••••••"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </div>
            {error && (
              <p role="alert" className="text-destructive text-sm">
                {error}
              </p>
            )}
            <Button type="submit" disabled={isPending} size="lg" className="h-11 w-full">
              {isPending ? "Logging in…" : "Log in"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
