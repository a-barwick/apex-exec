public class Node {
    public Object next;
}

List<Object> left = new List<Object>();
left.add(left);
left.add(1);
System.debug(left);

List<Object> same = new List<Object>();
same.add(same);
same.add(1);
System.debug(left == same);

List<Object> different = new List<Object>();
different.add(different);
different.add(2);
System.debug(left == different);

try {
    JSON.serialize(left);
    System.debug('unreachable');
} catch (IllegalArgumentException error) {
    System.debug(error.getTypeName());
    System.debug(error.getMessage());
}

Set<Object> values = new Set<Object>();
values.add(values);
System.debug(values);

Map<String, Object> mapped = new Map<String, Object>();
mapped.put('self', mapped);
System.debug(mapped);

Node node = new Node();
node.next = node;
System.debug(node);
