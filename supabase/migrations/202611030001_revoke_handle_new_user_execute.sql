-- handle_new_user() is a trigger function; it must not be callable through the REST RPC API.
revoke execute on function public.handle_new_user() from public, anon, authenticated;
