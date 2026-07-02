using System;

namespace StsStub.Services;

/// A market-data feed. Publishers hand out an event; subscribers must detach.
public sealed class QuoteService
{
    public event EventHandler<decimal>? QuoteReceived;
    public event EventHandler? Disconnected;

    public void Push(decimal price) => QuoteReceived?.Invoke(this, price);
}
