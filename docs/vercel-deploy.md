# Deploy do painel na Vercel (somente leitura / sem execução)

Somente `apps/web` (Next.js) roda na Vercel. A API Rust, o worker (WebSocket contínuo) e o
`gmx-adapter` são serviços de longa duração e devem rodar em um host de containers
(Fly.io, Railway, Render etc.) usando as imagens Docker do repositório.

## Configuração do projeto Vercel

- Root Directory: `apps/web`
- Framework: Next.js (detectado via `apps/web/vercel.json`)
- Install: `npm ci` · Build: `npm run build`

## Variáveis de ambiente (Production e Preview)

| Variável | Valor |
| --- | --- |
| `NEXT_PUBLIC_SUPABASE_URL` | URL do projeto Supabase |
| `NEXT_PUBLIC_SUPABASE_ANON_KEY` | chave **anon** (nunca a service role) |
| `NEXT_PUBLIC_API_URL` | URL HTTPS pública da API Rust (não `localhost`) |

Nunca configure `SUPABASE_SERVICE_ROLE_KEY`, chaves privadas ou `EXECUTION_ENABLED=true`
no projeto da Vercel. Essas variáveis pertencem somente ao backend.

## Ordem recomendada

1. Criar o projeto Supabase e aplicar `supabase/migrations/202611020001_initial_multiuser_schema.sql`.
2. Subir API, worker e gmx-adapter no host de containers e anotar a URL da API.
3. Definir `CORS_ORIGIN` da API com o domínio da Vercel.
4. Criar o projeto na Vercel com as variáveis acima e fazer o deploy.

A execução de ordens continua desabilitada. Veja `docs/validation-status.md` (release gate).
