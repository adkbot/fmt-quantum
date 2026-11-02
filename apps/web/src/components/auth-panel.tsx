"use client";

import { FormEvent, useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

export function AuthPanel() {
  const [email, setEmail] = useState("");
  const [signedInEmail, setSignedInEmail] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [apiStatus, setApiStatus] = useState("API ainda não verificada");

  useEffect(() => {
    if (!supabase) return;

    async function verifyApiSession(accessToken: string) {
      const apiUrl = process.env.NEXT_PUBLIC_API_URL;
      if (!apiUrl) {
        setApiStatus("Configure NEXT_PUBLIC_API_URL para validar a sessão na API.");
        return;
      }
      try {
        const response = await fetch(`${apiUrl.replace(/\/$/, "")}/v1/me`, {
          headers: { Authorization: `Bearer ${accessToken}` }
        });
        if (!response.ok) {
          setApiStatus(`API ainda não autenticou esta sessão (HTTP ${response.status}).`);
          return;
        }
        const identity = (await response.json()) as { user_id: string };
        setApiStatus(`API autenticada · usuário ${identity.user_id.slice(0, 8)}…`);
      } catch {
        setApiStatus("API indisponível ou ainda não iniciada.");
      }
    }

    void supabase.auth.getSession().then(({ data }) => {
      const session = data.session;
      setSignedInEmail(session?.user.email ?? null);
      if (session) void verifyApiSession(session.access_token);
    });

    const { data } = supabase.auth.onAuthStateChange((_event, session) => {
      setSignedInEmail(session?.user.email ?? null);
      if (session) void verifyApiSession(session.access_token);
      else setApiStatus("Faça login para validar a sessão na API.");
    });

    return () => data.subscription.unsubscribe();
  }, []);

  async function sendMagicLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !email.trim()) return;

    setBusy(true);
    setMessage("");
    const { error } = await supabase.auth.signInWithOtp({
      email: email.trim(),
      options: { emailRedirectTo: window.location.origin }
    });
    setBusy(false);
    setMessage(error ? error.message : "Link de acesso enviado. Verifique seu e-mail.");
  }

  async function signOut() {
    if (!supabase) return;
    setBusy(true);
    const { error } = await supabase.auth.signOut();
    setBusy(false);
    setMessage(error ? error.message : "Sessão encerrada.");
  }

  return (
    <section className="auth-panel">
      <div className="eyebrow">ACESSO MULTIUSUÁRIO</div>
      {signedInEmail ? (
        <>
          <strong className="auth-email">{signedInEmail}</strong>
          <p className="muted-copy">Sessão Supabase autenticada neste navegador.</p>
          <button className="button secondary" onClick={signOut} disabled={busy}>
            {busy ? "AGUARDE..." : "SAIR DA CONTA"}
          </button>
        </>
      ) : (
        <>
          <p className="muted-copy">
            Entre com seu e-mail. O Supabase enviará um link de acesso sem exigir senha.
          </p>
          <form className="auth-form" onSubmit={sendMagicLink}>
            <input
              type="email"
              autoComplete="email"
              placeholder="seu@email.com"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
              disabled={!supabase || busy}
            />
            <button className="button primary" type="submit" disabled={!supabase || busy}>
              {busy ? "ENVIANDO..." : "ENVIAR LINK"}
            </button>
          </form>
          {!supabase ? (
            <p className="auth-warning">Configure NEXT_PUBLIC_SUPABASE_URL e NEXT_PUBLIC_SUPABASE_ANON_KEY para habilitar o acesso.</p>
          ) : null}
        </>
      )}
      <p className="api-status">{apiStatus}</p>
      {message ? <p className="auth-message">{message}</p> : null}
    </section>
  );
}
