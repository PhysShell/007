"""First-party pilot fixture (Scope N1). Real doctest output."""


def shout(word):
    """Uppercase a word and add an exclamation mark.

    >>> shout('ok')
    'OK!'
    >>> shout('hi')
    'HI!'
    """
    return word.upper() + "!"
