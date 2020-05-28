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

    def warnings?
      @entries.any?(&:warning?)
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
      error(
        "Cannot reassign immutable local variable #{name.inspect}",
        location
      )
    end

    def reassign_immutable_attribute_error(name, location)
      error("Cannot reassign immutable attribute #{name.inspect}", location)
    end

    def reassign_undefined_local_error(name, location)
      error(
        "Cannot reassign undefined local variable #{name.inspect}",
        location
      )

      TypeSystem::Error.new
    end

    def reassign_undefined_attribute_error(name, location)
      error("Cannot reassign undefined attribute #{name}", location)

      TypeSystem::Error.new
    end

    def redefine_existing_local_error(name, location)
      error("The local variable #{name} has already been defined", location)

      TypeSystem::Error.new
    end

    def undefined_local_error(name, location)
      error("The local variable #{name} is undefined", location)
    end

    def redefine_existing_attribute_error(name, location)
      error("The attribute #{name} has already been defined", location)

      TypeSystem::Error.new
    end

    def redefine_existing_constant_error(name, location)
      error("The constant #{name} has already been defined", location)

      TypeSystem::Error.new
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

      TypeSystem::Error.new
    end

    def undefined_constant_error(name, location)
      error("The constant #{name} is undefined", location)

      TypeSystem::Error.new
    end

    def unknown_raw_instruction_error(name, location)
      error("The raw instruction #{name} does not exist", location)
    end

    def reopen_invalid_object_error(name, location)
      error("Cannot reopen #{name} since it's not an object", location)

      TypeSystem::Error.new
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

      TypeSystem::Error.new
    end

    def return_type_error(expected, found, location)
      exname = expected.type_name.inspect
      fname = found.type_name.inspect

      error(
        "Expected a value of type #{exname} to be returned instead of #{fname}",
        location
      )
    end

    def too_many_type_parameters(max, given, location)
      params = max == 1 ? 'parameter' : 'parameters'
      were = given == 1 ? 'is' : 'are'

      error(
        "This method takes up to #{max} type #{params}, " \
          "but #{given} #{were} given",
        location
      )

      TypeSystem::Error.new
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

      TypeSystem::Error.new
    end

    def type_parameter_count_error(given, exp, location)
      error(
        "This type requires #{exp} type parameters, but #{given} were given",
        location
      )

      TypeSystem::Error.new
    end

    def undefined_keyword_argument_error(name, receiver, method, location)
      mname = method.name.inspect
      tname = receiver.type_name.inspect
      aname = name.inspect

      error(
        "The message #{mname} for type #{tname} does not support " \
          "an argument with the name #{aname}",
        location
      )

      TypeSystem::Error.new
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
          'method does not define a type to throw',
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

    def redundant_try_warning(location)
      warn('This expression will never throw a value', location)
    end

    def define_instance_attribute_error(name, location)
      error(
        "Instance attributes such as #{name.inspect} can only be " \
          'defined in a constructor method',
        location
      )

      TypeSystem::Error.new
    end

    def import_undefined_symbol_error(mname, sname, location)
      error("The module #{mname} does not define #{sname.inspect}", location)
    end

    def import_existing_symbol_error(sname, location)
      error(
        "The symbol #{sname.inspect} can not be imported as it already exists",
        location
      )
    end

    def invalid_type_parameters(type, given, location)
      name = type.name.inspect
      ex = type.type_parameters.map(&:name).join(', ')
      got = given.join(', ')

      error(
        "The type #{name} requires type parameters [#{ex}] instead of [#{got}]",
        location
      )
    end

    def shadowing_type_parameter_error(name, location)
      error(
        "The type parameter #{name} shadows another type parameter with the " \
          'same name',
        location
      )
    end

    def dereference_error(type, location)
      tname = type.type_name

      error("The type #{tname} is not an optional type", location)
    end

    def method_requirement_error(receiver, block_type, value_type, bound, loc)
      rname = receiver.type_name.inspect
      bname = block_type.type_name.inspect
      vname = value_type.type_name.inspect
      req = bound.required_traits.map(&:type_name).join(', ')

      error(
        "The method #{bname} for #{rname} is only available when #{vname} " \
          "implements the following trait(s): #{req}",
        loc
      )
    end

    def invalid_type_parameter_requirement(type, location)
      error(
        "The type #{type.type_name.inspect} can not be used as a " \
          'type parameter requirement because it is not a trait',
        location
      )
    end

    def undefined_type_parameter_error(type, name, location)
      tname = type.type_name.inspect

      error(
        "The type #{tname} does not define the type parameter #{name.inspect}",
        location
      )

      TypeSystem::Error.new
    end

    def return_outside_of_method_error(location)
      error('The "return" keyword can only be used inside a method', location)
    end

    def invalid_cast_error(from, to, location)
      fname = from.type_name.inspect
      tname = to.type_name.inspect

      error("The type #{fname} can not be casted to #{tname}", location)

      TypeSystem::Error.new
    end

    def incompatible_optional_method(rec_type, nil_type, name, location)
      rec_impl = rec_type.lookup_method(name).type.type_name.inspect
      nil_impl = nil_type.lookup_method(name).type.type_name.inspect
      nname = nil_type.type_name.inspect
      rname = rec_type.type_name.inspect

      error(
        "The message #{name.inspect} can not be sent to a #{rname} " \
          "because its implementation (#{rec_impl}) is not compatible with " \
          "the implementation of #{nname} (#{nil_impl})",
        location
      )

      TypeSystem::Error.new
    end

    def unassigned_attribute(name, location)
      error(
        "The #{name.inspect} attribute must be assigned a value in this method",
        location
      )
    end

    def argument_type_missing(location)
      error(
        'You must provide an explicit type or default value for this argument',
        location
      )
    end
  end
end
