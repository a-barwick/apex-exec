// Simulate a deterministic retry policy with a do-while loop.
Integer attempt = 0;
Integer maxAttempts = 5;
Boolean connected = false;

do {
    attempt++;

    if (attempt < 3) {
        continue;
    }

    connected = true;
} while (!connected && attempt < maxAttempts);

String result = 'failed';
if (connected) {
    result = 'connected';
}

System.debug('attempts=' + attempt);
System.debug('result=' + result);
