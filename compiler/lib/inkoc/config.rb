# frozen_string_literal: true

module Inkoc
  class Config
    CACHE_NAME = 'inko'

    # The name of the directory to store bytecode files in.
    BYTECODE_DIR = 'bytecode'

    # The file extension of bytecode files.
    BYTECODE_EXT = '.inkoc'

    # The file extension of source files.
    SOURCE_EXT = '.inko'

    # The name of the root module for the standard library.
    STD_MODULE = 'std'

    # The name of the root module for the core library.
    CORE_MODULE = 'core'

    # The path to the bootstrap module.
    BOOTSTRAP_MODULE = 'bootstrap'

    # The path to the prelude module.
    PRELUDE_MODULE = 'prelude'

    MARKER_MODULE = 'std::marker'

    OBJECT_CONST = 'Object'
    TRAIT_CONST = 'Trait'
    ARRAY_CONST = 'Array'
    BLOCK_CONST = 'Block'
    INTEGER_CONST = 'Integer'
    FLOAT_CONST = 'Float'
    STRING_CONST = 'String'
    TRUE_CONST = 'True'
    FALSE_CONST = 'False'
    BOOLEAN_CONST = 'Boolean'
    NIL_CONST = 'Nil'
    FILE_CONST = 'File'
    BYTE_ARRAY_CONST = 'ByteArray'
    ARRAY_TYPE_PARAMETER = 'T'
    OPTIONAL_CONST = 'Optional'
    ANY_TRAIT_CONST = 'Any'

    MODULE_TYPE = 'Module'
    SELF_TYPE = 'Self'
    NEVER_TYPE = 'Never'
    MODULES_ATTRIBUTE = 'Modules'

    # The name of the constant to use as the receiver for raw instructions.
    RAW_INSTRUCTION_RECEIVER = '_INKOC'
    NEW_MESSAGE = 'new'
    UNKNOWN_MESSAGE_MESSAGE = 'unknown_message'
    UNKNOWN_MESSAGE_TRAIT = 'UnknownMessage'
    UNKNOWN_MESSAGE_MODULE = 'std::unknown_message'
    SET_INDEX_MESSAGE = '[]='
    MODULE_GLOBAL = 'ThisModule'
    CALL_MESSAGE = 'call'
    PANIC_MESSAGE = 'panic'
    TO_STRING_MESSAGE = 'to_string'
    MODULE_SEPARATOR = '::'
    BLOCK_TYPE_NAME = 'do'
    LAMBDA_TYPE_NAME = 'lambda'
    BLOCK_NAME = '<block>'
    LAMBDA_NAME = '<lambda>'
    TRY_BLOCK_NAME = '<try>'
    ELSE_BLOCK_NAME = '<else>'
    IMPL_NAME = '<impl>'
    OBJECT_NAME_INSTANCE_ATTRIBUTE = '@_object_name'
    IMPLEMENTED_TRAITS_INSTANCE_ATTRIBUTE = '@_implemented_traits'
    INIT_MESSAGE = 'init'

    RESERVED_CONSTANTS = Set.new(
      [
        MODULE_GLOBAL,
        RAW_INSTRUCTION_RECEIVER,
        SELF_TYPE,
        NEVER_TYPE,
      ]
    ).freeze

    DEFAULT_SOURCE_DIRECTORY = '/usr/lib/inko'

    RUNTIME_DIRECTORY = 'runtime'

    MAXIMUM_METHOD_ARGUMENTS = 255

    attr_reader :source_directories, :mode, :target

    def self.core_module_name(name)
      "#{CORE_MODULE}#{MODULE_SEPARATOR}#{name}"
    end

    def initialize(mode = :debug)
      @source_directories = Set.new
      @mode = mode
      @target = nil

      set_default_target_directory
      add_default_source_directories
    end

    def set_default_target_directory
      self.target = File.join(cache_directory, BYTECODE_DIR, mode.to_s)
    end

    def cache_directory
      if (env = ENV['INKOC_CACHE'])
        env
      else
        xdg_home = ENV['XDG_CACHE_HOME']

        cache_home =
          if xdg_home && !xdg_home.empty?
            xdg_home
          else
            File.join(Dir.home, '.cache')
          end

        File.join(cache_home, CACHE_NAME)
      end
    end

    def add_default_source_directories
      directory = ENV['INKOC_HOME']
      directory = DEFAULT_SOURCE_DIRECTORY if directory.nil? || directory.empty?
      source = File.join(directory, RUNTIME_DIRECTORY)

      add_source_directories([source])
    end

    def target=(path)
      @target = Pathname.new(path).expand_path
    end

    def release_mode?
      @mode == :release
    end

    def add_source_directories(directories)
      directories.each do |dir|
        @source_directories << Pathname.new(File.expand_path(dir))
      end
    end

    def create_directories
      @target.mkpath
    end
  end
end
