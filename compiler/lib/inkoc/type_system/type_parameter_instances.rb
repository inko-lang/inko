# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class TypeParameterInstances
      attr_reader :mapping

      def initialize
        @mapping = {}
      end

      def values
        @mapping.values
      end

      def [](param)
        @mapping[param]
      end

      def define(param, instance)
        @mapping[param] = instance
      end

      def empty?
        @mapping.empty?
      end

      def ==(other)
        other.is_a?(self.class) && mapping == other.mapping
      end

      def merge!(other)
        mapping.merge!(other.mapping)
        self
      end

      def dup
        self.class.new.merge!(self)
      end

      def with_rigid_type_parameters
        self.class.new.tap do |copy|
          @mapping.each do |param, instance|
            copy.define(param, instance.with_rigid_type_parameters)
          end
        end
      end
    end
  end
end
