# frozen_string_literal: true

module Inkoc
  module PrototypeID
    INTEGER = 0
    FLOAT = 1
    STRING = 2
    ARRAY = 3
    BLOCK = 4
    BOOLEAN = 5
    BYTE_ARRAY = 6
    NIL = 7
    MODULE = 8
    FFI_LIBRARY = 9
    FFI_FUNCTION = 10
    FFI_POINTER = 11
    IP_SOCKET = 12
    UNIX_SOCKET = 13
    PROCESS = 14
    READ_ONLY_FILE = 15
    WRITE_ONLY_FILE = 16
    READ_WRITE_FILE = 17
    HASHER = 18
    GENERATOR = 19
    TRAIT = 20
    CHILD_PROCESS = 21
  end
end
