// Process three batches, skipping item two and stopping each batch after item four.
Integer batches = 0;
Integer processed = 0;

while (batches < 3) {
    batches++;
    Integer item = 0;

    while (true) {
        item++;

        if (item == 2) {
            continue;
        }
        if (item > 4) {
            break;
        }

        processed++;
    }
}

if (processed != 9) {
    return;
}

System.debug('batches=' + batches);
System.debug('processed=' + processed);
System.debug('result=complete');
