Integer factorial(Integer value) {
    if (value <= 1) {
        return 1;
    }
    return value * factorial(value - 1);
}

String kind(String value) {
    return 'String';
}

String kind(Object value) {
    return 'Object';
}

String guardedDivide(Integer dividend, Integer divisor) {
    try {
        return 'quotient=' + (dividend / divisor);
    } catch (MathException error) {
        return 'division failed';
    } finally {
        System.debug('division finished');
    }
}

System.debug(FaCtOrIaL(5));
System.debug(KIND('Apex'));

Object boxed = 7;
System.debug(kind(boxed));

System.debug(guardedDivide(8, 2));
System.debug(guardedDivide(8, 0));
