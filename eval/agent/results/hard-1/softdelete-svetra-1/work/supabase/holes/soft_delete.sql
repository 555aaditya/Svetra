update "public"."entry"
   set "deleted_at" = now()
 where "id" = p_entry_id
   and "deleted_at" is null;
