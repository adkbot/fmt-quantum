"use client";

import { ReactNode, useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";
import { AuthPanel } from "@/components/auth-panel";

type GateState = "loading" | "signed-out" | "signed-in";

export function AuthGate({ children }: { children: ReactNode }) {
  const [state, setState] = useState<GateState>("loading");

  useEffect(() => {
    if (!supabase) {
      setState("signed-out");
      return;
    }
    void supabase.auth.getSession().then(({ data }) => {
      setState(data.session ? "signed-in" : "signed-out");
    });
    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setState(session ? "signed-in" : "signed-out");
    });
    return () => data.subscription.unsubscribe();
  }, []);

  if (state === "signed-in") return <>{children}</>;

  return (
    <main className="shell login-shell">
      <div className="brand login-brand">
        <div className="brand-mark">FQ</div>
        <div>
          <div className="brand-name">FMT QUANTUM</div>
          <div className="brand-subtitle">NON-CUSTODIAL TRADING SYSTEM</div>
        </div>
      </div>
      <article className="panel login-card">
        <h2>{state === "loading" ? "Carregando…" : "Acesse sua conta"}</h2>
        {state === "loading" ? null : <AuthPanel />}
      </article>
    </main>
  );
}
