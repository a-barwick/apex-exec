public class AuditService {
    public static String entry(String message) {
        return 'audit:' + message;
    }
}
