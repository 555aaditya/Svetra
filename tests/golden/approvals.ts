import { z } from "zod";

export const Doc = z.object({
  id: z.string().uuid(),
  author: z.string().uuid(),
  reviewer: z.string().uuid(),
  title: z.string(),
  status: z.enum(["draft", "submitted", "approved", "rejected"]),
});
export type Doc = z.infer<typeof Doc>;
