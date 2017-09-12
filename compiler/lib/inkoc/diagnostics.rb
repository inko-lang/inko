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
      @entries << Diagnostic.warning(message, location)
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
      error('Constants can not be defined as mutable', location)
    end

    def module_not_found_error(name, location)
      error("The module #{name} could not be found", location)
    end

    def reassign_immutable_local_error(name, location)
      error("Cannot reassign immutable local variable #{name}", location)
    end

    def reassign_undefined_local_error(name, location)
      error("Cannot reassign undefined local variable #{name}", location)
    end

    def redefine_existing_local_error(name, location)
      error("The local variable #{name} has already been defined", location)
    end

    def undefined_local_error(name, location)
      error("The local variable #{name} is undefined", location)
    end

    def redefine_existing_attribute_error(name, location)
      error("The attribute #{name} has already been defined", location)
    end

    def redefine_existing_constant_error(name, location)
      error("The constant #{name} has already been defined", location)
    end

    def undefined_attribute_error(name, location)
      error("The attribute #{name} is undefined", location)
    end

    def undefined_method_error(receiver, name, location)
      tname = receiver.type_name.inspect
      msg = name.inspect

      error(
        "The type #{tname} does not respond to the message #{msg}",
        location
      )
    end

    def undefined_constant_error(name, location)
      error("The constant #{name} is undefined", location)
    end

    def unknown_raw_instruction_error(name, location)
      error("The raw instruction #{name} does not exist", location)
    end

    def unreachable_code_warning(location)
      warn('This and any following expressions are unreachable', location)
    end

    def reopen_invalid_object_error(name, location)
      error("Cannot reopen #{name} since it's not an object", location)
    end

    def redefine_trait_error(location)
      error('Traits can not be reopened', location)
    end

    def define_required_method_on_non_trait_error(location)
      error('Required methods can only be defined on traits', location)
    end

    def type_error(expected, found, location)
      exp_name = expected.type_name
      found_name = found.type_name

      error(
        "Expected a value of type #{exp_name} instead of #{found_name}",
        location
      )
    end

    def type_parameters_error(exp, found, location)
      params = exp == 1 ? 'parameter' : 'parameters'
      were = found == 1 ? 'is' : 'are'

      error(
        "This type requires #{exp} type #{params}, but #{found} #{were} given",
        location
      )
    end

    def invalid_compiler_option(key, location)
      error("#{key} is not a valid compiler option", location)
    end
  end
end
