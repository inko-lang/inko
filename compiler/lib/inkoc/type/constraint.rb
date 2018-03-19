# frozen_string_literal: true

module Inkoc
  module Type
    class Constraint
      include Inspect
      include Predicates
      include TypeCompatibility

      attr_reader :required_methods, :inferred_type, :resolved

      def initialize
        @required_methods = {}
        @resolved = false
        @inferred_type = nil
      end

      # Resolves the current constraint to the given type.
      #
      # If the type could be resolved completely this method returns true,
      # otherwise false is returned.
      #
      # It's possible for a type to be partially resolved, though in this case
      # false will be returned.
      def infer_to(self_type, type)
        return false if self == type
        return resolved if inferred_type

        @inferred_type = type
        @resolved = required_methods.all? do |_, required|
          found = type.lookup_method(required.name)
          found.any? ? required.infer_to(self_type, found.type) : false
        end
      end

      def required_methods_implemented?(type)
        required_methods.all? do |_, required|
          type.implements_method?(required)
        end
      end

      def define_required_method(receiver, name, arguments, typedb)
        block = Type::Block.new(
          name: name,
          prototype: typedb.block_type,
          block_type: :method,
          returns: self.class.new,
          throws: self.class.new
        )

        block.define_self_argument(receiver)

        arguments.each_with_index do |arg, index|
          block.define_argument(index.to_s, arg)
        end

        required_methods[name] = block

        block.define_call_method
        block
      end

      def message_return_type(name)
        if inferred_type
          inferred_type.message_return_type(name)
        else
          Type::Dynamic.new
        end
      end

      def unresolved_constraint?
        !@resolved
      end

      def resolve_type(*)
        inferred_type || self
      end

      def type_compatible?(other)
        real_type = real_type_for_type_compatibility(other)

        if self == real_type
          true
        elsif real_type.dynamic?
          true
        elsif inferred_type
          inferred_type_compatibility?(other)
        else
          required_methods_implemented?(other)
        end
      end
      alias strict_type_compatible? type_compatible?

      def real_type_for_type_compatibility(other)
        if other.optional?
          other.type
        else
          other
        end
      end

      def inferred_type_compatibility?(other)
        other = type_for_type_compatibility(other)

        inferred_type.type_compatible?(other)
      end

      def type_for_type_compatibility(other)
        if other.constraint? && other.resolved
          other.inferred_type
        else
          other
        end
      end

      def implements_trait?(trait)
        if inferred_type
          inferred_type.implements_trait?(trait)
        else
          # This may seem odd (and probably would if it were not for the hacky
          # type system) but this allows us to infer a constraint to a type
          # parameter or trait.
          true
        end
      end

      def responds_to_message?(name)
        if inferred_type
          inferred_type.responds_to_message?(name)
        else
          false
        end
      end

      def constraint?
        true
      end

      def dereference?
        if inferred_type
          inferred_type.dereference?
        else
          true
        end
      end

      def infer_as_optional?
        inferred_type&.optional?
      end

      def dereferenced_type
        inferred_type&.dereferenced_type || self
      end

      def initialize_as(*)
        self
      end

      def type_name
        if inferred_type
          inferred_type.type_name
        else
          '?'
        end
      end

      def with_method_requirements(*)
        self
      end
    end
  end
end
