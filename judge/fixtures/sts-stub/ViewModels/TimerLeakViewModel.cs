using System;
using System.Threading;

namespace StsStub.ViewModels;

// REAL leak: creates a Timer, keeps it in a field, never disposes it.
public sealed class TimerLeakViewModel
{
    private readonly Timer _timer;

    public TimerLeakViewModel()
    {
        _timer = new Timer(Tick, null, 0, 1000);   // created, never Dispose()d
    }

    private void Tick(object? state)
    {
        // poll
    }
}
