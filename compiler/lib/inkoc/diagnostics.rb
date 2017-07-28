# frozen_string_literal: true

module Inkoc
  class Diagnostics
    include Enumerable

    attr_reader :entries

    def initialize
      @entries = []
    end

    def error(message, location)
      @entries << Diagnostic.error(message, location)
    end

    def warn(message, location)
      @entries << Diagnostic.warn(message, location)
    end

    def length
      @entries.length
    end

    def errors?
      @entries.any?(&:error?)
    end

    def each(&block)
      @entries.each(&block)
    end

    def mutable_constant_error(location)
      error('constants can not be defined as mutable', location)
    end

    def module_not_found_error(name, location)
      error("the module #{name} could not be found", location)
    end

    def reassign_immutable_local_error(name, location)
      error("cannot reassign immutable local variable #{name}", location)
    end

    def reassign_undefined_local_error(name, location)
      error("cannot reassign undefined local variable #{name}", location)
    end

    def redefine_existing_local_error(name, location)
      error("the local variable #{name} has already been defined", location)
    end

    def redefine_existing_attribute_error(name, location)
      error("the attribute #{name} has already been defined", location)
    end

    def undefined_attribute_error(name, location)
      error("the attribute #{name} is undefined", location)
    end

    def undefined_method_error(name, location)
      error("the method #{name} is undefined", location)
    end

    def not_a_method_error(name, location)
      error("#{name} is not a valid method", location)
    end

    def undefined_constant_error(name, location)
      error("the constant #{name} is undefined", location)
    end

    def unknown_raw_instruction_error(name, location)
      error("the raw instruction #{name} does not exist", location)
    end
  end
end
