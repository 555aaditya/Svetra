  insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values (p_person, 'member', p_team)
    on conflict do nothing;
