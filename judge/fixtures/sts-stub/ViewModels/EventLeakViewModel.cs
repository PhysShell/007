using System;
using StsStub.Services;

namespace StsStub.ViewModels;

// REAL leak: subscribes to QuoteReceived, never detaches. No IDisposable.
public sealed class EventLeakViewModel
{
    private readonly QuoteService _service;

    public EventLeakViewModel(QuoteService service)
    {
        _service = service;
        _service.QuoteReceived += OnQuote;   // subscription, no matching -=
    }

    private void OnQuote(object? sender, decimal price)
    {
        // update UI
    }
}
