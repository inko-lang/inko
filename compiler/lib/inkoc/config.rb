# frozen_string_literal: true

module Inkoc
  class Config
    PROGRAM_NAME = 'inkoc'

    # The name of the directory to store bytecode files in.
    BYTECODE_DIR = 'bytecode'

    # The file extension of bytecode files.
    BYTECODE_EXT = '.inkoc'

    # The file extension of source files.
    SOURCE_EXT = '.inko'

    # The name of the root module for the standard library.
    STD_MODULE = 'std'

    # The path to the bootstrap module.
    BOOTSTRAP_MODULE = 'bootstrap'

    # The path to the prelude module.
    PRELUDE_MODULE = 'prelude'

    OBJECT_BUILTIN = '_Object'
    TRAIT_BUILTIN = '_Trait'
    ARRAY_BUILTIN = '_Array'
    HASH_MAP_BUILTIN = '_HashMap'

    MODULE_TYPE = 'Module'
    MODULES_ATTRIBUTE = 'Modules'

    # The name of the constant to use as the receiver for raw instructions.
    RAW_INSTRUCTION_RECEIVER = '_INKOC'

    NEW_MESSAGE = 'new'
    DEFINE_REQUIRED_METHOD_MESSAGE = 'define_required_method'
    CALL_MESSAGE = 'call'
    SELF_LOCAL = 'self'
    MODULE_SEPARATOR = '::'

    attr_reader :source_directories, :mode, :target

    def initialize
      @source_directories = Set.new
      @mode = :debug
      @target = Pathname
        .new(File.join(SXDG::XDG_CACHE_HOME, PROGRAM_NAME, BYTECODE_DIR))
    end

    def target=(path)
      @target = Pathname.new(path).expand_path
    end

    def release_mode?
      @mode == :release
    end

    def release_mode
      @mode = :release
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
