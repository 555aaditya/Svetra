begin
  update "public"."doc" set "title" = p_title
   where "id" = p_doc and "author" = p_caller and "status" = 'draft';
  if not found then
    raise exception 'rename_doc: document is not an editable draft' using errcode = '42501';
  end if;
end
