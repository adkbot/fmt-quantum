The browser client uses only the Supabase project URL and anon/public key. The service-role key must never be added to frontend environment variables or bundled into the browser.

Email magic-link auth is a first-pass identity layer. Wallet ownership still requires a separate server-verified signature challenge before a wallet is associated with the account. The Rust API must validate the Supabase JWT on every authenticated request.
