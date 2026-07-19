Integer index, weight = 3, total;

for (index = 0, weight = 3; index < 3; index++, weight--) {
    total = (total ?? 0) + weight;
}

String unset;
System.debug(total);
System.debug(unset);
