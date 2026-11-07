import { z } from "zod";

export const AuditLog = z.object({
  id: z.string().uuid(),
  note: z.string().nullable(),
});
export type AuditLog = z.infer<typeof AuditLog>;

export const Post = z.object({
  id: z.string().uuid(),
  author: z.string().uuid(),
  body: z.string().nullable(),
});
export type Post = z.infer<typeof Post>;

/** The one client method holes need. A Supabase client has it. */
export type Rpc = {
  rpc(fn: string, args: Record<string, unknown>): PromiseLike<{ data: unknown; error: unknown }>;
};

/** Calls the `write_audit` hole. Throws the database's error if the call is refused. */
export async function write_audit(client: Rpc): Promise<void> {
  const { error } = await client.rpc("write_audit", {});
  if (error) throw error;
}
