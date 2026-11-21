import { z } from "zod";

export const Invite = z.object({
  id: z.string().uuid(),
  owner: z.string().uuid(),
  email: z.string(),
  status: z.enum(["pending", "accepted", "expired"]).nullable(),
  expires_at: z.string().datetime({ offset: true }),
});
export type Invite = z.infer<typeof Invite>;
