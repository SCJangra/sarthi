-- pandoc filter for the help file: a link to a heading, `[text](#tag)`, becomes its text followed by a link to the
-- heading's help tag, `text |tag|`, since each heading's id is its tag.
function Link(link)
  if link.target:sub(1, 1) == '#' then
    local tag = pandoc.RawInline('vimdoc', '|' .. link.target:sub(2) .. '|')
    if pandoc.utils.stringify(link.content) == link.target:sub(2) then
      return tag
    end
    local inlines = link.content
    inlines:insert(pandoc.Space())
    inlines:insert(tag)
    return inlines
  end
end
