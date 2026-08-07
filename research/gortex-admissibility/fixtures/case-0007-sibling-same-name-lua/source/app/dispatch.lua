local Invoice = require("billing.invoice")
local MailMessage = require("mail.message")

local M = {}

--- Receiver arrives as an untyped parameter: not decidable locally.
function M.dispatch_invoice(invoice)
  return invoice:send()
end

--- Receiver arrives as an untyped parameter: not decidable locally.
function M.dispatch_mail(message)
  return message:send()
end

--- Receiver is constructed in this scope: decidable by local dataflow.
function M.dispatch_new(id)
  local invoice = Invoice.new(id)
  return invoice:send()
end

return M
