import { z } from "zod";

export const Team = z.object({
  id: z.string().uuid(),
  name: z.string(),
});
export type Team = z.infer<typeof Team>;

export const Task = z.object({
  id: z.string().uuid(),
  team: z.string().uuid(),
  author: z.string().uuid(),
  title: z.string(),
});
export type Task = z.infer<typeof Task>;

/** The one client method holes need. A Supabase client has it. */
export type Rpc = {
  rpc(fn: string, args: Record<string, unknown>): PromiseLike<{ data: unknown; error: unknown }>;
};

export const StartTeamInput = z.object({
  name: z.string(),
});
export type StartTeamInput = z.infer<typeof StartTeamInput>;

/** Calls the `start_team` hole. Throws the database's error if the call is refused. */
export async function start_team(client: Rpc, input: StartTeamInput): Promise<Team> {
  const { data, error } = await client.rpc("start_team", StartTeamInput.parse(input));
  if (error) throw error;
  return Team.parse(data);
}

export const AddMemberInput = z.object({
  team: z.string().uuid(),
  person: z.string().uuid(),
});
export type AddMemberInput = z.infer<typeof AddMemberInput>;

/** Calls the `add_member` hole. Throws the database's error if the call is refused. */
export async function add_member(client: Rpc, input: AddMemberInput): Promise<void> {
  const { error } = await client.rpc("add_member", AddMemberInput.parse(input));
  if (error) throw error;
}
