// Build a billing summary while skipping a non-billable line.
Integer subtotal = 0;
Integer billableLines = 0;

for (Integer lineNumber = 1; lineNumber <= 6; lineNumber++) {
    if (lineNumber == 3) {
        continue;
    }

    Integer lineAmount = lineNumber * 125;
    subtotal = subtotal + lineAmount;
    billableLines++;
}

Boolean priorityAccount = subtotal >= 2000 && billableLines == 5;
String status = 'standard';
if (priorityAccount) {
    status = 'priority';
}

System.debug('billableLines=' + billableLines);
System.debug('subtotal=' + subtotal);
System.debug('status=' + status);
