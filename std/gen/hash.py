# Code taken from https://github.com/ilanschnell/perfect-hash/, licensed as
# follows:
#
# BSD 3-Clause License
#
# Copyright (c) 2019 - 2025, Ilan Schnell
# All rights reserved.
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are met:
#     * Redistributions of source code must retain the above copyright
#       notice, this list of conditions and the following disclaimer.
#     * Redistributions in binary form must reproduce the above copyright
#       notice, this list of conditions and the following disclaimer in the
#       documentation and/or other materials provided with the distribution.
#     * Neither the name of the Ilan Schnell nor the
#       names of its contributors may be used to endorse or promote products
#       derived from this software without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
# ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
# WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
# DISCLAIMED. IN NO EVENT SHALL ILAN SCHNELL BE LIABLE FOR ANY
# DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
# (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
# LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND
# ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
# (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
# SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
#
#!/usr/bin/env python
"""
Generate a minimal perfect hash function for the keys in a file,
desired hash values may be specified within this file as well.
A given code template is filled with parameters, such that the
output is code which implements the hash function.
Templates can easily be constructed for any programming language.

The code is based on an a program A.M. Kuchling wrote:
http://www.amk.ca/python/code/perfect-hash

The algorithm the program uses is described in the paper
'Optimal algorithms for minimal perfect hashing',
Z. J. Czech, G. Havas and B.S. Majewski.
http://cmph.sourceforge.net/papers/chm92.pdf

The algorithm works like this:

1.  You have K keys, that you want to perfectly hash against some
    desired hash values.

2.  Choose a number N larger than K.  This is the number of
    vertices in a graph G, and also the size of the resulting table G.

3.  Pick two random hash functions f1, f2, that return values from 0..N-1.

4.  Now, for all keys, you draw an edge between vertices f1(key) and f2(key)
    of the graph G, and associate the desired hash value with that edge.

5.  If G is cyclic, go back to step 2.

6.  Assign values to each vertex such that, for each edge, you can add
    the values for the two vertices and get the desired (hash) value
    for that edge.  This task is easy, because the graph is acyclic.
    This is done by picking a vertex, and assigning it a value of 0.
    Then do a depth-first search, assigning values to new vertices so that
    they sum up properly.

7.  f1, f2, and vertex values of G now make up a perfect hash function.


For simplicity, the implementation of the algorithm combines steps 5 and 6.
That is, we check for loops in G and assign the vertex values in one procedure.
If this procedure succeeds, G is acyclic and the vertex values are assigned.
If the procedure fails, G is cyclic, and we go back to step 2, replacing G
with a new graph, and thereby discarding the vertex values from the failed
attempt.
"""
import sys
import random
import string
import subprocess
import shutil
import tempfile
from collections import defaultdict
from io import StringIO
from os.path import join


__version__ = "0.5.2"

anum_chars = string.ascii_letters + string.digits
verbose = False
trials = 50


class Graph(object):
    """
    Implements a graph with 'N' vertices.  First, you connect the graph with
    edges, which have a desired value associated.  Then the vertex values
    are assigned, which will fail if the graph is cyclic.  The vertex values
    are assigned such that the two values corresponding to an edge add up to
    the desired edge value (mod N).
    """
    def __init__(self, N):
        self.N = N                     # number of vertices

        # maps a vertex number to the list of tuples (vertex, edge value)
        # to which it is connected by edges.
        self.adjacent = defaultdict(list)

    def connect(self, vertex1, vertex2, edge_value):
        """
        Connect 'vertex1' and 'vertex2' with an edge, with associated
        value 'value'
        """
        # Add vertices to each other's adjacent list
        self.adjacent[vertex1].append((vertex2, edge_value))
        self.adjacent[vertex2].append((vertex1, edge_value))

    def assign_vertex_values(self):
        """
        Try to assign the vertex values, such that, for each edge, you can
        add the values for the two vertices involved and get the desired
        value for that edge, i.e. the desired hash key.
        This will fail when the graph is cyclic.

        This is done by a Depth-First Search of the graph.  If the search
        finds a vertex that was visited before, there's a loop and False is
        returned immediately, i.e. the assignment is terminated.
        On success (when the graph is acyclic) True is returned.
        """
        self.vertex_values = self.N * [-1]  # -1 means unassigned

        visited = self.N * [False]

        # Loop over all vertices, taking unvisited ones as roots.
        for root in range(self.N):
            if visited[root]:
                continue

            # explore tree starting at 'root'
            self.vertex_values[root] = 0    # set arbitrarily to zero

            # Stack of vertices to visit, a list of tuples (parent, vertex)
            tovisit = [(None, root)]
            while tovisit:
                parent, vertex = tovisit.pop()
                visited[vertex] = True

                # Loop over adjacent vertices, but skip the vertex we arrived
                # here from the first time it is encountered.
                skip = True
                for neighbor, edge_value in self.adjacent[vertex]:
                    if skip and neighbor == parent:
                        skip = False
                        continue

                    if visited[neighbor]:
                        # We visited here before, so the graph is cyclic.
                        return False

                    tovisit.append((vertex, neighbor))

                    # Set new vertex's value to the desired edge value,
                    # minus the value of the vertex we came here from.
                    self.vertex_values[neighbor] = (
                        edge_value - self.vertex_values[vertex]) % self.N

        # check if all vertices have been assigned
        for vertex in range(self.N):
            assert self.vertex_values[vertex] >= 0

        # We got though, so the graph is acyclic,
        # and all values are now assigned.
        return True


class StrSaltHash(object):
    """
    Random hash function generator.
    Simple byte level hashing: each byte is multiplied to another byte from
    a random string of characters, summed up, and finally modulo NG is
    taken.
    """
    def __init__(self, N):
        self.N = N
        self.salt = bytearray()

    def __call__(self, key):
        key = key.encode()
        while len(self.salt) < len(key):  # add more salt as necessary
            self.salt.append(random.choice(anum_chars.encode()))

        return sum(self.salt[i] * c for i, c in enumerate(key)) % self.N

    template = """
def hash_f(key, salt):
    return sum(salt[i] * c for i, c in enumerate(key)) % $NG

def perfect_hash(key):
    key = key.encode()
    if len(key) > $NS:
        return -1
    return (G[hash_f(key, b"$S1")] +
            G[hash_f(key, b"$S2")]) % $NG
"""

class IntSaltHash(object):
    """
    Random hash function generator.
    Simple byte level hashing, each byte is multiplied in sequence to a table
    containing random numbers, summed tp, and finally modulo NG is taken.
    """
    def __init__(self, N):
        self.N = N
        self.salt = []

    def __call__(self, key):
        key = key.encode()
        while len(self.salt) < len(key):  # add more salt as necessary
            self.salt.append(random.randrange(1, self.N))

        return sum(self.salt[i] * c for i, c in enumerate(key)) % self.N

    template = """
S1 = [$S1]
S2 = [$S2]
assert len(S1) == len(S2) == $NS

def hash_f(key, salt):
    return sum(salt[i] * c for i, c in enumerate(key)) % $NG

def perfect_hash(key):
    key = key.encode()
    if len(key) > $NS:
        return -1
    return (G[hash_f(key, S1)] + G[hash_f(key, S2)]) % $NG
"""

def builtin_template(Hash):
    return """\
# =======================================================================
# ================= Python code for perfect hash function ===============
# =======================================================================

G = [$G]
""" + Hash.template + """
# ============================ Sanity check =============================

K = [$K]
assert len(K) == $NK

for h, k in enumerate(K):
    assert perfect_hash(k) == h
"""


class TooManyInterationsError(Exception):
    pass


def generate_hash(keys, Hash=StrSaltHash, pow2=False):
    """
    Return hash functions f1 and f2, and G for a perfect minimal hash.
    Input is an iterable of 'keys', whos indicies are the desired hash values.
    'Hash' is a random hash function generator, that means Hash(N) returns a
    returns a random hash function which returns hash values from 0..N-1.
    """
    if not isinstance(keys, (list, tuple)):
        raise TypeError("list or tuple expected")
    NK = len(keys)
    if NK != len(set(keys)):
        raise ValueError("duplicate keys")
    for key in keys:
        if not isinstance(key, str):
            raise TypeError("key a not string: %r" % key)
    if NK > 10000 and Hash == StrSaltHash:
        print("""\
WARNING: You have %d keys.
         Using --hft=1 is likely to fail for so many keys.
         Please use --hft=2 instead.
""" % NK)

    # the number of vertices in the graph G
    if pow2:
        NG = 1
        while NG < NK + 1:
            NG *= 2
    else:
        NG = NK + 1

    if verbose:
        print('NG = %d' % NG)

    trial = 0  # Number of trial graphs so far
    while True:
        if (trial % trials) == 0:   # trials failures, increase NG slightly
            if trial > 0:
                if pow2:
                    NG *= 2
                else:
                    NG = max(NG + 1, int(1.05 * NG))
            if verbose:
                sys.stdout.write('\nGenerating graphs NG = %d ' % NG)
        trial += 1

        if NG > 100 * (NK + 1):
            raise TooManyInterationsError("%d keys" % NK)

        if verbose:
            sys.stdout.write('.')
            sys.stdout.flush()

        G = Graph(NG)   # Create graph with NG vertices
        f1 = Hash(NG)   # Create 2 random hash functions
        f2 = Hash(NG)

        # Connect vertices given by the values of the two hash functions
        # for each key.  Associate the desired hash value with each edge.
        for hashval, key in enumerate(keys):
            G.connect(f1(key), f2(key), hashval)

        # Try to assign the vertex values.  This will fail when the graph
        # is cyclic.  But when the graph is acyclic it will succeed and we
        # break out, because we're done.
        if G.assign_vertex_values():
            break

    if verbose:
        print('\nAcyclic graph found after %d trials.' % trial)
        print('NG = %d' % NG)

    # Sanity check the result by actually verifying that all the keys
    # hash to the right value.
    for hashval, key in enumerate(keys):
        assert hashval == (
            G.vertex_values[f1(key)] + G.vertex_values[f2(key)]
        ) % NG

    if verbose:
        print('OK')

    return f1, f2, G.vertex_values


class Format(object):

    def __init__(self, width=76, indent=4, delimiter=', '):
        self.width = width
        self.indent = indent
        self.delimiter = delimiter

    def print_format(self):
        print("Format options:")
        for name in 'width', 'indent', 'delimiter':
            print('  %s: %r' % (name, getattr(self, name)))

    def __call__(self, data, quote=False):
        if isinstance(data, bytearray):
            return data.decode()

        if not isinstance(data, (list, tuple)):
            return data

        lendel = len(self.delimiter)
        aux = StringIO()
        pos = 20
        for i, elt in enumerate(data):
            last = bool(i == len(data) - 1)

            s = ('"%s"' if quote else '%s') % elt

            if pos + len(s) + lendel > self.width:
                aux.write('\n' + (self.indent * ' '))
                pos = self.indent

            aux.write(s)
            pos += len(s)
            if not last:
                aux.write(self.delimiter)
                pos += lendel

        return aux.getvalue()


def generate_code(keys, Hash=StrSaltHash, template=None, options=None,
                  pow2=False):
    """
    Takes a list of key value pairs and inserts the generated parameter
    lists into the 'template' string.  'Hash' is the random hash function
    generator, and the optional keywords are formating options.
    The return value is the substituted code template.
    """
    f1, f2, G = generate_hash(keys, Hash, pow2)

    assert f1.N == f2.N == len(G)
    try:
        salt_len = len(f1.salt)
        assert salt_len == len(f2.salt)
    except TypeError:
        salt_len = None

    if template is None:
        template = builtin_template(Hash)

    if options is None:
        fmt = Format()
    else:
        fmt = Format(width=options.width, indent=options.indent,
                     delimiter=options.delimiter)

    if verbose:
        fmt.print_format()

    res = string.Template(template).substitute(
        NS = salt_len,
        S1 = fmt(f1.salt),
        S2 = fmt(f2.salt),
        NG = len(G),
        G  = fmt(G),
        NK = len(keys),
        K  = fmt(list(keys), quote=True))

    if pow2:
        res = res.replace("%% %d" % len(G), "& %d" % (len(G) - 1))

    return res


def read_table(filename, options):
    """
    Reads keys and desired hash value pairs from a file.  If no column
    for the hash value is specified, a sequence of hash values is generated,
    from 0 to N-1, where N is the number of rows found in the file.
    """
    if verbose:
        print("Reading table from file `%s' to extract keys." % filename)
    try:
        fi = open(filename)
    except IOError:
        sys.exit("Error: Could not open `%s' for reading." % filename)

    keys = []

    if verbose:
        print("Reader options:")
        for name in 'comment', 'splitby', 'keycol':
            print('  %s: %r' % (name, getattr(options, name)))

    for n, line in enumerate(fi):
        line = line.strip()
        if not line or line.startswith(options.comment):
            continue

        if line.count(options.comment): # strip content after comment
            line = line.split(options.comment)[0].strip()

        row = [col.strip() for col in line.split(options.splitby)]

        try:
            key = row[options.keycol - 1]
        except IndexError:
            sys.exit("%s:%d: Error: Cannot read key, not enough columns." %
                     (filename, n + 1))

        keys.append(key)

    fi.close()

    if not keys:
        exit("Error: no keys found in file `%s'." % filename)

    return keys


def read_template(path):
    if verbose:
        print("Reading template from path: %r" % path)

    with open(path, 'r') as fi:
        return fi.read()


def run_code(code):
    tmpdir = tempfile.mkdtemp()
    path = join(tmpdir, 't.py')
    with open(path, 'w') as fo:
        fo.write(code)
    try:
        subprocess.check_call([sys.executable, path])
    except subprocess.CalledProcessError as e:
        raise AssertionError(e)
    finally:
        shutil.rmtree(tmpdir)


def main():
    import argparse

    description = """\
Generates code for perfect hash functions from
a file with keywords and a code template.
If no template file is provided, a small built-in Python template
is processed and the output code is written to stdout.
"""

    p = argparse.ArgumentParser(
        description=description,
        formatter_class=argparse.ArgumentDefaultsHelpFormatter)

    p.add_argument('KEYS_FILE', nargs=1,
                   help="file containing keys for perfect hash function")

    p.add_argument('TMPL_FILE', nargs="?",
                   help="code template file, must contain `tmpl` in filename")

    p.add_argument("--delimiter", action="store", default=", ",
                   help="Delimiter for list items used in output.",
                   metavar="STR")

    p.add_argument("--indent", action="store", default=4, type=int,
                   help="Make INT spaces at the beginning of a "
                        "new line when generated list is wrapped.",
                   metavar="INT")

    p.add_argument("--width", action="store", default=76, type=int,
                   help="Maximal width of generated list when wrapped.",
                   metavar="INT")

    p.add_argument("--comment", action="store", default="#",
                   help="STR is the character, or sequence of characters, "
                        "which marks the beginning of a comment (which runs "
                        " till the end of the line), in the input KEYS_FILE.",
                   metavar="STR")

    p.add_argument("--splitby", action="store", default = ",",
                   help="STR is the character by which the columns "
                        "in the input KEYS_FILE are split.",
                   metavar="STR")

    p.add_argument("--keycol", action="store", default=1, type=int,
                   help="Specifies the column INT in the input "
                        "KEYS_FILE which contains the keys.",
                   metavar="INT")

    p.add_argument("--trials", action="store", default=50, type=int,
                   help="Specifies the number of trials before NG is "
                        "increased.  A small INT will give compute faster, "
                        "but the array G will be large.  A large INT will "
                        "take longer to compute but G will be smaller.",
                   metavar="INT")

    p.add_argument("--hft", action="store", default=1, type=int,
                   help="Hash function type INT.  Possible values "
                        "are 1 (StrSaltHash) and 2 (IntSaltHash).",
                   metavar="INT")

    p.add_argument("--pow2", action="store_true",
                   help="Only use powers of 2 for graph size NG.")

    p.add_argument("-e", "--execute", action="store_true",
                   help="execute generated code within Python interpreter")

    p.add_argument("-o", "--output", action="store",
                   help="Specify output FILE explicitly. "
                        "`-o std` means standard output. "
                        "`-o no` means no output. "
                        "By default, the file name is obtained "
                        "from the name of the template file by "
                        "substituting `tmpl` with `code`.",
                   metavar="FILE")

    p.add_argument("-v", "--verbose", action="store_true")
    p.add_argument("-V", "--version", action="version",
                   version="perfect-hash version: %s" % __version__)

    args = p.parse_args()

    if args.trials <= 0:
        p.error("trials before increasing N has to be larger than zero")

    global trials, verbose
    trials = args.trials
    verbose = args.verbose

    if args.TMPL_FILE and 'tmpl' not in args.TMPL_FILE:
        p.error("template filename does not contain 'tmpl'")

    if args.hft == 1:
        Hash = StrSaltHash
    elif args.hft == 2:
        Hash = IntSaltHash
    else:
        p.error("Hash function %s not implemented." % args.hft)

    # --------------------- end parsing and checking --------------

    keys_file = args.KEYS_FILE[0]
    if verbose:
        print("keys_file = %r" % keys_file)

    keys = read_table(keys_file, args)
    if verbose:
        print("Number of keys: %d" % len(keys))

    tmpl_file = args.TMPL_FILE
    if verbose:
        print("tmpl_file = %r" % tmpl_file)

    template = read_template(tmpl_file) if tmpl_file else None

    if args.output:
        outname = args.output
    elif tmpl_file:
        if 'tmpl' not in tmpl_file:
            sys.exit("Hmm, template filename does not contain 'tmpl'")
        outname = tmpl_file.replace('tmpl', 'code')
    else:
        outname = 'std'

    if verbose:
        print("outname = %r\n" % outname)

    code = generate_code(keys, Hash, template, args, args.pow2)

    if outname == 'std':
        sys.stdout.write(code)
    elif outname != 'no':
        with open(outname, 'w') as fo:
            fo.write(code)

    if args.execute:
        if verbose:
            print('Executing code...\n')
        run_code(code)


if __name__ == '__main__':
    main()
