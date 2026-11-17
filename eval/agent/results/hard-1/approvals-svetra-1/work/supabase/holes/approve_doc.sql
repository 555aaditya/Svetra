begin
  update "public"."doc" set "status" = 'approved'
   where "id" = p_doc and "reviewer" = p_caller and "author" <> p_caller and "status" = 'submitted';
  if not found then
    raise exception 'approve_doc: document is not submitted for your review' using errcode = '42501';
  end if;
end
