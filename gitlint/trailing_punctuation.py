from gitlint.rules import LineRule, RuleViolation, CommitMessageTitle


class TrailingPunctuation(LineRule):
    name = 'inko-subject-trailing-punctuation'
    id = 'U1'
    target = CommitMessageTitle
    violation_message = "Commit subjects can't have trailing punctuation ({0})"

    def validate(self, subject, _commit):
        # We allow trailing question marks because they may be used for method
        # names (e.g. 'Add Foo.bar?').
        punctuation_marks = ':!.,;'

        for punctuation_mark in punctuation_marks:
            if subject.endswith(punctuation_mark):
                return [
                    RuleViolation(
                        self.id,
                        self.violation_message.format(punctuation_mark),
                        subject
                    )
                ]
