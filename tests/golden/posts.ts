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
