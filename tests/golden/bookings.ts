import { z } from "zod";

export const Location = z.object({
  id: z.string().uuid(),
  name: z.string(),
});
export type Location = z.infer<typeof Location>;

export const Booking = z.object({
  id: z.string().uuid(),
  customer: z.string().uuid(),
  location: z.string().uuid(),
  starts: z.string().datetime({ offset: true }),
});
export type Booking = z.infer<typeof Booking>;
