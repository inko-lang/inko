from pygments.lexer import RegexLexer, words, bygroups
from pygments import token

__version__ = "1.1.0"

KEYWORDS = (
    "and",
    "as",
    "async",
    "break",
    "builtin",
    "case",
    "class",
    "do",
    "else",
    "enum",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "loop",
    "match",
    "move",
    "mut",
    "next",
    "or",
    "pub",
    "recover",
    "ref",
    "return",
    "self",
    "static",
    "throw",
    "trait",
    "true",
    "try",
    "uni",
    "while",
)


class InkoLexer(RegexLexer):
    name = "Inko"
    aliases = ["inko"]
    filenames = ["*.inko"]

    tokens = {
        "root": [
            (r"#.*$", token.Comment.Single),
            ('"', token.String.Double, "dstring"),
            ("'", token.String.Single, "sstring"),
            (r"_?[A-Z]\w*", token.Name.Constant),
            (r"@_?\w+", token.Name.Variable.Instance),
            (r"(?i)-?0x[0-9a-f_]+", token.Number.Integer),
            (r"(?i)-?[\d_]+\.\d+(e[+-]?\d+)?", token.Number.Float),
            (r"(?i)-?[\d_]+(e[+-]?\d+)?", token.Number.Integer),
            (r"(\w+)(::)", bygroups(token.Name.Namespace, token.Text)),
            (r"\w+:", token.String.Symbol),
            (r"(->|!!)", token.Keyword),
            (r"((<|>|\+|-|\/|\*)=?|==)", token.Operator),
            ("try!", token.Keyword),
            ("import", token.Keyword.Namespace),
            ("let", token.Keyword.Declaration),
            (words(KEYWORDS, suffix=r"\b"), token.Keyword),
            (r"!|\?|\}|\{|\[|\]|\.|,|:|\(|\)|=", token.Punctuation),
            (r"\w+\b", token.Text),
            (r"\s+", token.Whitespace),
        ],
        "dstring": [
            (r'[^"\\]+', token.String.Double),
            (r"\\.", token.String.Escape),
            ('"', token.String.Double, "#pop"),
        ],
        "sstring": [
            (r"[^'\\]+", token.String.Single),
            (r"\\.", token.String.Escape),
            ("'", token.String.Single, "#pop"),
        ],
    }
