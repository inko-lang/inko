# frozen_string_literal: true

module Inkoc
  module TypeLookup
    def type_for_constant(node, sources)
      name = node.name

      if node.receiver
        receiver = type_for_constant(node.receiver, sources)
        sources = [receiver] + sources
      end

      sources.each do |source|
        type = source.lookup_type(name)

        return initialize_type(type, node, sources) if type
      end

      diagnostics.undefined_constant_error(node.name, node.location)

      Type::Dynamic.new
    end

    def initialize_type(type, node, sources)
      return type if type.type_parameter?

      exp_len = type.type_parameters.length
      got_len = node.type_parameters.length
      instance = type.new_instance

      if exp_len != got_len
        diagnostics.type_parameters_error(exp_len, got_len, node.location)

        return Type::Dynamic.new
      end

      param_names = type.type_parameter_names

      node.type_parameters.each_with_index do |arg, index|
        arg_type = type_for_constant(arg, sources)

        instance.init_type_parameter(param_names[index], arg_type)
      end

      instance
    end
  end
end
