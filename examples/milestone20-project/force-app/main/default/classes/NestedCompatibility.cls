public class NestedCompatibility {
    public enum State {
        Ready,
        Running,
        Done
    }

    public interface Scorer {
        Integer score();
    }

    public virtual class BaseJob {
        public Integer weight;

        public BaseJob(Integer weight) {
            this.weight = weight;
        }

        public virtual Integer score() {
            return weight;
        }
    }

    public class Job extends BaseJob implements Scorer {
        public Integer bonus = 1;
        {
            bonus += 2;
        }

        public Job() {
            super(4);
        }

        public override Integer score() {
            return weight + bonus;
        }
    }

    public class LocalFailure extends Exception {}

    public static Integer initializationCount = 0;
    static {
        initializationCount += 1;
    }

    public static String run() {
        Scorer scorer = new Job();
        State state = State.valueOf('Running');
        Type jobType = Job.class;
        return state.name()
            + '|' + state.ordinal()
            + '|' + NestedCompatibility.State.values().size()
            + '|' + scorer.score()
            + '|' + jobType
            + '|' + initializationCount;
    }

    public static String catchCustomFailure() {
        try {
            throw new LocalFailure('typed');
        } catch (LocalFailure failure) {
            return failure.getTypeName() + ':' + failure.getMessage();
        }
    }
}
