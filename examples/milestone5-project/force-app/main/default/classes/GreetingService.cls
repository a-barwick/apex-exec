public class GreetingService implements Greeting {
    private String name;

    public GreetingService(String name) {
        this.name = name;
    }

    public String message() {
        return 'Hello, ' + name + '!';
    }
}
