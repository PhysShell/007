local Invoice = {}
Invoice.__index = Invoice

function Invoice.new(id)
  return setmetatable({ id = id }, Invoice)
end

--- Marks the invoice dispatched and returns the audit id.
function Invoice:send()
  return "invoice:" .. self.id
end

return Invoice
