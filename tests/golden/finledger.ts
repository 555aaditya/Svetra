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

export const Account = z.object({
  id: z.string().uuid(),
  owner: z.string().uuid(),
  currency: z.enum(["USD", "EUR", "GBP", "INR"]),
  balance: decimal(18, 6),
  status: z.enum(["active", "frozen", "closed"]).nullable(),
  created_at: z.string().datetime({ offset: true }).nullable(),
});
export type Account = z.infer<typeof Account>;

export const Transaction = z.object({
  id: z.string().uuid(),
  source: z.string().uuid(),
  destination: z.string().uuid(),
  amount: decimal(18, 6),
  status: z.enum(["pending", "committed", "failed", "reversed"]).nullable(),
  created_at: z.string().datetime({ offset: true }).nullable(),
});
export type Transaction = z.infer<typeof Transaction>;

/** The one client method holes need. A Supabase client has it. */
export type Rpc = {
  rpc(fn: string, args: Record<string, unknown>): PromiseLike<{ data: unknown; error: unknown }>;
};

export const TransferInput = z.object({
  source: z.string().uuid(),
  destination: z.string().uuid(),
  amount: decimal(18, 6),
});
export type TransferInput = z.infer<typeof TransferInput>;

/** Calls the `transfer` hole. Throws the database's error if the call is refused. */
export async function transfer(client: Rpc, input: TransferInput): Promise<Transaction> {
  const { data, error } = await client.rpc("transfer", TransferInput.parse(input));
  if (error) throw error;
  return Transaction.parse(data);
}
