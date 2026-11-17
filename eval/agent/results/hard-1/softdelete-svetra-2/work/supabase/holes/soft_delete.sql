  -- Mark the entry as deleted. The owner check is already done by the hole's `when`.
  -- An entry that is already deleted keeps its original deletion time.
  update "public"."entry"
     set "deleted_at" = now()
   where "id" = p_entry_id
     and "deleted_at" is null;
