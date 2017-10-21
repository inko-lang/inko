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

    def reassign_undefined_attribute_error(name, location)
      error("Cannot reassign undefined attribute #{name}", location)
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

    def undefined_attribute_error(receiver, name, location)
      tname = receiver.type_name.inspect

      error(
        "The type #{tname} does not define the attribute #{name.inspect}",
        location
      )
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
      exp_name = expected.type_name.inspect
      found_name = found.type_name.inspect

      error(
        "Expected a value of type #{exp_name} instead of #{found_name}",
        location
      )
    end

    def return_type_error(expected, found, location)
      exname = expected.type_name.inspect
      fname = found.type_name.inspect

      error(
        "Expected a value of type #{exname} to be returned instead of #{fname}",
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

    def uninplemented_trait_error(trait, object, required_trait, location)
      tname = trait.type_name.inspect
      oname = object.type_name.inspect
      rname = required_trait.type_name.inspect

      error(
        "The trait #{tname} can not be implemented for the type #{oname} " \
          "because it does not implement the trait #{rname}",
        location
      )
    end

    def unimplemented_method_error(method, object, location)
      mname = method.type_name.inspect
      oname = object.type_name.inspect

      error(
        "The method #{mname} must be implemented by type #{oname}",
        location
      )
    end

    def generated_trait_not_implemented_error(trait, type, location)
      missing = []
      name = type.type_name.inspect

      trait.required_traits.each do |t|
        missing << t.type_name unless type.implements_trait?(t)
      end

      traits = missing.join(', ')

      error(
        "The type #{name} does not implement the following trait(s): #{traits}",
        location
      )
    end

    def argument_count_error(given, range, location)
      given_word = given == 1 ? 'was' : 'were'

      exp_word, exp_val =
        if given < range.min
          ['requires', range.min]
        else
          ['takes up to', range.max]
        end

      arg_word = exp_val == 1 ? 'argument' : 'arguments'

      error(
        "This message #{exp_word} #{exp_val} #{arg_word} " \
          "but #{given} #{given_word} given",
        location
      )
    end

    def undefined_keyword_argument_error(name, type, location)
      tname = type.type_name.inspect

      error(
        "The type #{tname} does not define the argument #{name.inspect}",
        location
      )
    end

    def redefine_reserved_constant_error(name, location)
      error(
        "The reserved constant #{name.inspect} cannot be redefined",
        location
      )
    end

    def throw_without_throw_defined_error(type, location)
      tname = type.type_name.inspect

      error(
        "cannot throw a value of type #{tname} because the enclosing " \
          'block does not define a type to throw',
        location
      )
    end

    def throw_at_top_level_error(type, location)
      tname = type.type_name.inspect

      error("cannot throw a value of type #{tname} at the top-level", location)
    end

    def missing_throw_error(throw_type, location)
      tname = throw_type.type_name.inspect

      error(
        "this block is expected to throw a value of type #{tname} " \
          'but no value is ever thrown',
        location
      )
    end

    def missing_try_error(throw_type, location)
      tname = throw_type.type_name.inspect

      error(
        "This message may throw a value of type #{tname} but the `try` " \
          'statement is missing',
        location
      )
    end
  end
end
