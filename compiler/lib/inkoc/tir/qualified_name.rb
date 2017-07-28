# frozen_string_literal: true

module Inkoc
  module TIR
    class QualifiedName
      attr_reader :parts

      def initialize(parts)
        if parts.empty?
          raise ArgumentError, 'Qualified names must contain at least 1 segment'
        end

        @parts = parts
      end

      def module_name
        @parts.last
      end

      def source_path_with_extension
        @parts.join(File::SEPARATOR) + Config::SOURCE_EXT
      end

      def to_s
        @parts.join(Config::MODULE_SEPARATOR)
      end

      def inspect
        "QualifiedName(#{self})"
      end
    end
  end
end
