from gitlint.rules import LineRule, RuleViolation, CommitMessageTitle


class SubjectCapital(LineRule):
    name = 'inko-subject-capital'
    id = 'INKO3'
    target = CommitMessageTitle
    violation_message = 'Commit subjects must start with a capital letter'

    def validate(self, subject, _commit):
        if len(subject) > 0 and subject[0].islower():
            return [
                RuleViolation(
                    self.id,
                    self.violation_message,
                    subject
                )
            ]
