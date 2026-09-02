local MailMessage = {}
MailMessage.__index = MailMessage

function MailMessage.new(to)
  return setmetatable({ to = to }, MailMessage)
end

--- Sibling `send` -- unrelated to Invoice:send beyond the name.
function MailMessage:send()
  return "mail:" .. self.to
end

return MailMessage
