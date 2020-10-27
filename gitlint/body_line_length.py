from gitlint.rules import LineRule, RuleViolation, CommitMessageBody, IntOption


class BodyLineLength(LineRule):
    name = 'inko-body-line-length'
    id = 'U2'
    target = CommitMessageBody
    options_spec = [IntOption('line-length', 72, "Max line length")]
    violation_message = "This line is too long ({0}>{1})"

    def validate(self, line, _commit):
        max = self.options['line-length'].value

        if len(line) <= max:
            return

        # URLs can extend beyond the line limit, as they can't be wrapped
        # without breaking the URL.
        if self.too_long_because_of_url(line):
            return

        return [
            RuleViolation(
                self.id,
                self.violation_message.format(len(line), max),
                line
            )
        ]

    def too_long_because_of_url(self, line):
        limit = self.options['line-length'].value

        if 'http:' not in line and 'https:' not in line:
            return False

        column = line.find('http')

        if column > limit:
            # If the URL already starts beyond the limit, we consider the line
            # invalid.
            return False

        max = len(line)

        # Determine if the URL ends beyond the line limit.
        while column < max:
            if line[column] == ' ':
                break
            column += 1

        return column > limit
