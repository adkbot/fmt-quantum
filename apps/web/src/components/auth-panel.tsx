"use client";

import { FormEvent, useEffect, useState } from "react";
import { supabase } from "@/lib/supabase";

export function AuthPanel() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [mode, setMode] = useState<"login" | "signup" | "reset">("login");
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

  async function submitAuth(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !email.trim()) return;

    setBusy(true);
    setMessage("");
    const cleanEmail = email.trim();

    if (mode === "signup") {
      if (password.length < 8) {
        setBusy(false);
        setMessage("A senha precisa ter pelo menos 8 caracteres.");
        return;
      }
      const { data, error } = await supabase.auth.signUp({
        email: cleanEmail,
        password,
        options: { emailRedirectTo: window.location.origin }
      });
      setBusy(false);
      if (error) {
        setMessage(error.message);
      } else if (data.session) {
        setMessage("Conta criada. Você já está logado.");
      } else {
        setMessage("Conta criada. Confirme seu e-mail pelo link que enviamos e depois faça login.");
        setMode("login");
        setPassword("");
      }
      return;
    }

    if (mode === "reset") {
      const { error } = await supabase.auth.resetPasswordForEmail(cleanEmail, {
        redirectTo: window.location.origin
      });
      setBusy(false);
      setMessage(error ? error.message : "Se o e-mail existir, enviamos o link para redefinir a senha.");
      return;
    }

    const { error } = await supabase.auth.signInWithPassword({ email: cleanEmail, password });
    setBusy(false);
    if (error) {
      setMessage(
        error.message.toLowerCase().includes("email not confirmed")
          ? "Confirme seu e-mail pelo link que enviamos antes de entrar."
          : error.message.toLowerCase().includes("invalid login")
            ? "E-mail ou senha incorretos."
            : error.message
      );
    } else {
      setPassword("");
      setMessage("");
    }
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
          <div className="auth-tabs" role="tablist">
            <button type="button" className={mode === "login" ? "auth-tab active" : "auth-tab"} onClick={() => { setMode("login"); setMessage(""); }}>ENTRAR</button>
            <button type="button" className={mode === "signup" ? "auth-tab active" : "auth-tab"} onClick={() => { setMode("signup"); setMessage(""); }}>CRIAR CONTA</button>
          </div>
          <p className="muted-copy">
            {mode === "signup"
              ? "Crie sua conta com e-mail e senha. Você receberá um e-mail para confirmar o cadastro."
              : mode === "reset"
                ? "Informe seu e-mail para receber o link de redefinição de senha."
                : "Entre com seu e-mail e senha."}
          </p>
          <form className="auth-form stacked" onSubmit={submitAuth}>
            <input
              type="email"
              autoComplete="email"
              placeholder="seu@email.com"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
              disabled={!supabase || busy}
            />
            {mode !== "reset" ? (
              <input
                type="password"
                autoComplete={mode === "signup" ? "new-password" : "current-password"}
                placeholder="Senha (mín. 8 caracteres)"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                required
                minLength={8}
                disabled={!supabase || busy}
              />
            ) : null}
            <button className="button primary" type="submit" disabled={!supabase || busy}>
              {busy
                ? "AGUARDE..."
                : mode === "signup"
                  ? "CRIAR CONTA"
                  : mode === "reset"
                    ? "ENVIAR LINK DE REDEFINIÇÃO"
                    : "ENTRAR"}
            </button>
          </form>
          {mode === "login" ? (
            <button type="button" className="auth-link" onClick={() => { setMode("reset"); setMessage(""); }}>Esqueci minha senha</button>
          ) : mode === "reset" ? (
            <button type="button" className="auth-link" onClick={() => { setMode("login"); setMessage(""); }}>Voltar ao login</button>
          ) : null}
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
