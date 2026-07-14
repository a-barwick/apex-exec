public class Entry {
    public static String run() {
        Greeting service = new GreetingService('Apex');
        return service.message();
    }
}
