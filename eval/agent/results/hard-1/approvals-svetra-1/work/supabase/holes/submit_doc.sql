begin
  update "public"."doc" set "status" = 'submitted'
   where "id" = p_doc and "author" = p_caller and "status" = 'draft';
  if not found then
    raise exception 'submit_doc: document is not a draft you wrote' using errcode = '42501';
  end if;
end
