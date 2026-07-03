using System;
using System.Threading;
using StsStub.Services;

namespace StsStub.ViewModels;

// FALSE POSITIVE: analyzer flags the subscription and the timer, but both are
// torn down in Dispose(). own-check missed the teardown.
public sealed class CleanViewModel : IDisposable
{
    private readonly QuoteService _service;
    private readonly Timer _timer;

    public CleanViewModel(QuoteService service)
    {
        _service = service;
        _service.QuoteReceived += OnQuote;          // flagged OWN001 (line ~17)
        _timer = new Timer(Tick, null, 0, 1000);    // flagged OWN-TIMER (line ~18)
    }

    private void OnQuote(object? sender, decimal price) { }
    private void Tick(object? state) { }

    public void Dispose()
    {
        _service.QuoteReceived -= OnQuote;
        _timer.Dispose();
    }
}
