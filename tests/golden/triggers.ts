import { z } from "zod";

/** An exact decimal. Accepts a string, or a number only while it's still exact; yields a string. */
const decimal = (precision: number, scale: number) =>
  z
    .union([
      z.string(),
      z.number().refine((n) => Number.isSafeInteger(Math.round(n * 10 ** scale)), {
        message: "Number too large to be exact in JavaScript; select this column as text (column::text).",
      }),
    ])
    .transform(String)
    .pipe(
      z
        .string()
        .regex(
          new RegExp(
            `^-?${precision === scale ? "0" : `\\d{1,${precision - scale}}`}${scale === 0 ? "" : `(\\.\\d{1,${scale}})?`}$`,
          ),
        ),
    );

export const Customer = z.object({
  id: z.string().uuid(),
  owner: z.string().uuid(),
  name: z.string(),
});
export type Customer = z.infer<typeof Customer>;

export const Order = z.object({
  id: z.string().uuid(),
  owner: z.string().uuid(),
  customer: z.string().uuid(),
  status: z.enum(["open", "paid", "shipped"]).nullable(),
  total: decimal(18, 2),
});
export type Order = z.infer<typeof Order>;

export const Notification = z.object({
  id: z.string().uuid(),
  owner: z.string().uuid(),
  message: z.string(),
});
export type Notification = z.infer<typeof Notification>;
