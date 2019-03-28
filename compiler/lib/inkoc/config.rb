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

    # The path to the module that defines the default globals exposed to every
    # module.
    GLOBALS_MODULE = 'globals'

    TRAIT_MODULE = 'trait'

    INTERNAL_TRAIT_IMPORT = '_inkoc_std_trait'
    HASH_MAP_LITERAL_RECEIVER = '_inkoc_hash_map_literal'

    MARKER_MODULE = 'std::marker'
    HASH_MAP_MODULE = 'std::hash_map'

    INKO_CONST = 'Inko'
    OBJECT_CONST = 'Object'
    TRAIT_CONST = 'Trait'
    ARRAY_CONST = 'Array'
    HASH_MAP_CONST = 'HashMap'
    BLOCK_CONST = 'Block'
    INTEGER_CONST = 'Integer'
    FLOAT_CONST = 'Float'
    STRING_CONST = 'String'
    TRUE_CONST = 'True'
    FALSE_CONST = 'False'
    BOOLEAN_CONST = 'Boolean'
    NIL_CONST = 'Nil'
    READ_ONLY_FILE_CONST = 'ReadOnlyFile'
    WRITE_ONLY_FILE_CONST = 'WriteOnlyFile'
    READ_WRITE_FILE_CONST = 'ReadWriteFile'
    HASHER_CONST = 'Hasher'
    BYTE_ARRAY_CONST = 'ByteArray'
    LIBRARY_CONST = 'Library'
    FUNCTION_CONST = 'Function'
    POINTER_CONST = 'Pointer'
    PROCESS_CONST = 'Process'
    ARRAY_TYPE_PARAMETER = 'T'
    OPTIONAL_CONST = 'Optional'

    MODULE_TYPE = 'Module'
    SELF_TYPE = 'Self'
    DYNAMIC_TYPE = 'Dynamic'
    VOID_TYPE = 'Void'
    MODULES_ATTRIBUTE = 'Modules'

    # The name of the constant to use as the receiver for raw instructions.
    RAW_INSTRUCTION_RECEIVER = '_INKOC'
    NEW_MESSAGE = 'new'
    UNKNOWN_MESSAGE_MESSAGE = 'unknown_message'
    UNKNOWN_MESSAGE_TRAIT = 'UnknownMessage'
    UNKNOWN_MESSAGE_MODULE = 'std::unknown_message'
    FROM_ARRAY_MESSAGE = 'from_array'
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
    IMPLEMENT_TRAIT_MESSAGE = 'implement'
    OBJECT_NAME_INSTANCE_ATTRIBUTE = '@_object_name'
    INIT_MESSAGE = 'init'

    RESERVED_CONSTANTS = Set.new(
      [
        MODULE_GLOBAL,
        RAW_INSTRUCTION_RECEIVER,
        SELF_TYPE,
        VOID_TYPE,
        DYNAMIC_TYPE
      ]
    ).freeze

    DEFAULT_SOURCE_DIRECTORY = '/usr/lib/inko'

    RUNTIME_DIRECTORY = 'runtime'

    attr_reader :source_directories, :mode, :target

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
