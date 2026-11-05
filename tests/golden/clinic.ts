import { z } from "zod";

export const Visit = z.object({
  id: z.string().uuid(),
  patient: z.string().uuid(),
  notes: z.string().nullable(),
});
export type Visit = z.infer<typeof Visit>;
