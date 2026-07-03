using System;
using StsStub.Services;

namespace StsStub.ViewModels;

// Two DISTINCT findings reported on the SAME (path,line,rule) with different
// messages -> distinct finding_ids, same tuple. Exercises positional pairing.
public sealed class MixedViewModel : IDisposable
{
    private readonly QuoteService _service;

    public MixedViewModel(QuoteService service)
    {
        _service = service;
        _service.QuoteReceived += OnQuote; _service.Disconnected += OnDown;  // two subs, one line
    }

    private void OnQuote(object? sender, decimal price) { }
    private void OnDown(object? sender, EventArgs e) { }

    // REAL: neither subscription is detached; no -= anywhere.
    public void Dispose()
    {
        // forgot to unsubscribe
    }
}
